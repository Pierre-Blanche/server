#

object storage

```
/
  sid
    {id1} -> (user,timestamp)
    {id2} -> (user,timestamp)
  otp
    email
      {encrypted_email_hash_1}
        {otp_token_1}
      {encrypted_email_hash_2}
      {encrypted_email_hash_3}
    sms
      {encrypted_sms_hash_1}
  pk
    {encrypted_email_hash_2}
      {pk_1}
```

### Sessions

```mermaid
graph TD;
  A[Request user page] --> B[Session cookie?];
  B -- Yes --> C[Extract username/email] --> D[Still valid?]
  D -- Yes --> J(("<b>Show<br>user page</b>"))
  D -- No --> G
  B -- No --> E["<i>Login page</i><br>Ask for username/email"] --> F[Passkey in db?]
  F -- Yes --> G[Ask for passkey]
  F -- No -->  M[Email OTP bound to username/email]
  M -- Visit --> H[Create and store passkey]
  H --> G
  G -- Success --> K[Session cookie] --> J
  G -- Failure --> N[Reset?]
  N -- Yes --> M
  N -- No --> G
```

Two session cookies are used, one for the server and one for javascript:

- `st`  
  contains the session expiration timestamp (millis)

- `sid`
  *http-only*<br>
  contains the session id

API requests return `403 FORBIDDEN` if `sid` is missing or expired.

For user scoped html pages, the server first checks if the `sid` exists and has user info, and if not redirects to the
login page.

The page itself should check the `st` cookie and if missing or expired, it should trigger the passkey auth.


