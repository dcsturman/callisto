# Local Development Setup

## Running Locally

### 1. Configure Backend for Local Development

In `callisto/Dockerfile`, ensure TLS is disabled by setting the build argument:
```dockerfile
ARG TLS_UPGRADE=0
```

### 2. Start the Backend

```bash
docker-compose up be
```

### 3. Start the Frontend

```bash
cd fe/callisto
export VITE_CALLISTO_BACKEND=http://localhost:30000
export VITE_NODE_SERVER=http://localhost:50001
npm start -- --port 50001
```

### 4. Access the Application

Open `http://localhost:50001` in your browser.

## Running Integration Tests

To run the integration tests, you need to generate TLS keys first:

```bash
cd callisto/keys
bash ../build_keys.sh
```

When prompted for certificate information, you can accept defaults by pressing Enter.

Then run the tests:

```bash
cd callisto
cargo test --release
```

## Troubleshooting

**WebSocket handshake fails:**
- Ensure `VITE_CALLISTO_BACKEND=http://localhost:30000` (not https)
- Backend logs should show successful WebSocket connections

**OAuth "redirect_uri_mismatch" error:**
- Verify `VITE_NODE_SERVER=http://localhost:50001`
- Check Google OAuth2 settings include `http://localhost:50001`
- Ensure frontend is actually running on port 50001

**OAuth "missing access_token" error:**
- This usually means redirect_uri mismatch
- Frontend and backend must use the same redirect_uri value

**Integration tests fail with "Connection refused":**
- Run `bash callisto/build_keys.sh` from the `callisto/keys` directory to regenerate TLS keys
