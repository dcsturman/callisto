# To-do

Cross-cutting items that do not belong to one feature. Ship-design and
weapon-schema gaps live in `callisto/modified designs.md` (TODOs 1-6).

---

## Stop re-requesting Google consent on every login (HIGH PRIORITY)

**Symptom:** every login produces a Google security email — *"callistoflight.com
requested access to your account information."* Signing in should reuse the grant
already given, silently.

**Cause.** `fe/callisto/src/components/scenarios/Authentication.tsx` asks for two
things that combine badly:

```ts
accessType: "offline",   // ask for a refresh token
prompt: "consent",       // force the consent screen every time
```

Google issues a refresh token only when consent is *freshly* granted, so
`prompt: "consent"` is what guarantees one comes back. A fresh grant is also
exactly what triggers the email.

The backend requires that token to be present:

```rust
// callisto/src/authentication.rs:1076
struct GoogleTokenResponse {
  refresh_token: String,   // not Option
  ...
}
```

Without `prompt: "consent"`, Google omits `refresh_token` on every login after the
first, deserialization fails, and login breaks. That is almost certainly why the
flag was added.

**But the field is never read.** Line 1076 is its only occurrence in the crate. It
is parsed and discarded. Callisto does not use Google refresh tokens — session
continuity comes from its own `callisto-session-key` cookie and `ValidateSession`.

**Fix, in this order:**

1. `refresh_token: String` -> `Option<String>` in `GoogleTokenResponse`, or drop the
   field. Do this first: it is what makes the frontend change safe.
2. Drop `accessType: "offline"` — no refresh token is wanted.
3. Drop `prompt: "consent"`.

Google then reuses the existing grant silently. A side benefit: the first-time
consent screen gets less alarming, since offline access is what makes Google warn
about reaching the account "while you're not using the app".

**Testing.** Auth is the one place a mistake locks us out of prod, so land it on
canary and confirm a full login there — including a *second* login, which is the
one that previously would have failed — before it goes near main. Watch for the
security email stopping, which is the actual acceptance criterion.
