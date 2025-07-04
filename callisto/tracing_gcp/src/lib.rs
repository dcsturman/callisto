use std::{io::Write, sync::Mutex};

use google_cloud_logging::{
    GCHttpMethod, GCHttpRequest, GCLogSeverity, GCSourceLocation, GoogleCloudStructLog,
};
use tracing::{
    field::Visit,
    span::{self, Id},
    Level, Subscriber,
};
use tracing_subscriber::{registry::LookupSpan, Layer};

pub struct GcpLayer<W: Write> {
    write: Mutex<W>,
}

impl<W: Write> GcpLayer<W> {
    pub fn init_with_writer(w: W) -> Self {
        Self {
            write: Mutex::new(w),
        }
    }
}

impl<S, W> Layer<S> for GcpLayer<W>
where
    W: Write + 'static,
    S: Subscriber + for<'span> LookupSpan<'span>,
{
    fn on_new_span(
        &self,
        attrs: &span::Attributes<'_>,
        id: &span::Id,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut ir = IR::default();
        attrs.values().record(&mut ir);

        if let Some(span) = _ctx.span(id) {
            span.extensions_mut().insert(ir);
        }
    }

    fn on_record(
        &self,
        id: &Id,
        values: &span::Record<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let Some(span) = _ctx.span(id) else {
            return;
        };

        let mut ext = span.extensions_mut();
        let Some(ir) = ext.get_mut::<IR>() else {
            return;
        };

        values.record(ir);
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: tracing_subscriber::layer::Context<'_, S>) {
        // this is the only fn that would benefit from RwLock
        let mut ir = IR::default();
        event.record(&mut ir);

        let mut log_entry = GoogleCloudStructLog {
            severity: severity(event),
            time: Some(chrono::offset::Utc::now()),
            source_location: Some(GCSourceLocation {
                file: event.metadata().file(),
                line: event.metadata().line().map(|line| line.to_string()),
                function: event.metadata().module_path(),
            }),
            ..Default::default()
        };
        ir.apply(&mut log_entry);

        // process parents
        if let Some(parent_spans) = ctx.event_scope(event) {
            for span in parent_spans {
                let ext = span.extensions();
                let Some(par_ir) = ext.get::<IR>() else {
                    continue;
                };
                par_ir.apply(&mut log_entry);
            }
        }

        // log_entry ready to be used
        // may make sense to use a queue and do this somewhere else in the future
        let log_entry = serde_json::to_string(&log_entry).unwrap();
        writeln!(self.write.lock().unwrap(), "{log_entry}").unwrap();
    }
}

#[derive(Default, Debug)]
struct IR {
    req_meth: Option<GCHttpMethod>,
    req_url: Option<String>,
    req_status: Option<u16>,
    req_ua: Option<String>,
    req_remote_ip: Option<String>,
    req_server_ip: Option<String>,
    req_latency: Option<String>,
    message: Option<String>,
    unknown_fields: Vec<(String, String)>,
}

impl Visit for IR {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        match field.name() {
            "http.method" => {
                let method = format!("{:?}", value).to_lowercase();
                self.req_meth = match method.as_ref() {
                    "\"post\"" => Some(GCHttpMethod::Post),
                    "\"get\"" => Some(GCHttpMethod::Get),
                    "\"put\"" => Some(GCHttpMethod::Put),
                    "\"head\"" => Some(GCHttpMethod::Head),
                    _ => None,
                }
            }
            "http.url" => {
                let url = format!("{:?}", value);
                let url = remove_quotes(url);
                self.req_url = Some(url);
            }
            "http.status" => {
                let status = format!("{:?}", value);
                let status = status.parse::<u16>();
                if let Ok(status) = status {
                    self.req_status = Some(status);
                }
            }
            "http.ua" => {
                let ua = format!("{:?}", value);
                let ua = remove_quotes(ua);
                self.req_ua = Some(ua);
            }
            "http.remote_ip" => {
                let ip = format!("{:?}", value);
                let ip = remove_quotes(ip);
                self.req_remote_ip = Some(ip);
            }
            "http.server_ip" => {
                let ip = format!("{:?}", value);
                let ip = remove_quotes(ip);
                self.req_server_ip = Some(ip);
            }
            "http.latency" => {
                let lat = format!("{:?}", value);
                let lat = remove_quotes(lat);
                self.req_latency = Some(lat);
            }
            "message" => {
                self.message = Some(format!("{:?}", value));
            }
            name => self
                .unknown_fields
                .push((name.to_string(), format!("{:?}", value))),
        }
    }
}

impl IR {
    fn apply(&self, log_entry: &mut GoogleCloudStructLog) {
        if let Some(meth) = self.req_meth {
            match &mut log_entry.http_request {
                Some(req) => {
                    req.request_method = Some(meth);
                }
                None => {
                    let req = GCHttpRequest {
                        request_method: Some(meth),
                        ..Default::default()
                    };
                    log_entry.http_request = Some(req);
                }
            }
        }

        if let Some(url) = self.req_url.clone() {
            match &mut log_entry.http_request {
                Some(req) => {
                    req.request_url = Some(url);
                }
                None => {
                    let req = GCHttpRequest {
                        request_url: Some(url),
                        ..Default::default()
                    };
                    log_entry.http_request = Some(req);
                }
            }
        }

        if let Some(status) = self.req_status {
            match &mut log_entry.http_request {
                Some(req) => {
                    req.status = Some(status);
                }
                None => {
                    let req = GCHttpRequest {
                        status: Some(status),
                        ..Default::default()
                    };
                    log_entry.http_request = Some(req);
                }
            }
        }

        if let Some(ua) = self.req_ua.clone() {
            match &mut log_entry.http_request {
                Some(req) => {
                    req.user_agent = Some(ua);
                }
                None => {
                    let req = GCHttpRequest {
                        user_agent: Some(ua),
                        ..Default::default()
                    };
                    log_entry.http_request = Some(req);
                }
            }
        }

        if let Some(ip) = self.req_remote_ip.clone() {
            match &mut log_entry.http_request {
                Some(req) => {
                    req.remote_ip = Some(ip);
                }
                None => {
                    let req = GCHttpRequest {
                        remote_ip: Some(ip),
                        ..Default::default()
                    };
                    log_entry.http_request = Some(req);
                }
            }
        }

        if let Some(ip) = self.req_server_ip.clone() {
            match &mut log_entry.http_request {
                Some(req) => {
                    req.server_ip = Some(ip);
                }
                None => {
                    let req = GCHttpRequest {
                        server_ip: Some(ip),
                        ..Default::default()
                    };
                    log_entry.http_request = Some(req);
                }
            }
        }

        if let Some(lat) = self.req_latency.clone() {
            match &mut log_entry.http_request {
                Some(req) => {
                    req.latency = Some(lat);
                }
                None => {
                    let req = GCHttpRequest {
                        latency: Some(lat),
                        ..Default::default()
                    };
                    log_entry.http_request = Some(req);
                }
            }
        }

        if let Some(ref message) = self.message {
            log_entry.message = Some(message.clone());
        }

        for (key, val) in &self.unknown_fields {
            log_entry.labels.insert(key.clone(), val.clone());
        }
    }
}

fn remove_quotes(s: String) -> String {
    if s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s
    }
}

fn severity(e: &tracing::Event<'_>) -> Option<GCLogSeverity> {
    match *e.metadata().level() {
        Level::TRACE => None,
        Level::DEBUG => Some(GCLogSeverity::Debug),
        Level::INFO => Some(GCLogSeverity::Info),
        Level::WARN => Some(GCLogSeverity::Warning),
        Level::ERROR => Some(GCLogSeverity::Error),
    }
}
