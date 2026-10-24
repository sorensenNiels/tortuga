Test certificates for `tests/tls.rs` (R-HTTP-16), made with `openssl` for these tests only:
`test-ca.pem` is a throwaway CA ("Tortuga Test CA"), and `localhost.pem` / `localhost.key` a server
certificate it signed for `localhost` and `127.0.0.1`. Never trust them anywhere else.

`xml-suite.postman_collection.json` is a trimmed copy of a real Postman test suite for an XML service, without its
credentials or hosts (`tests/postman.rs`, R-COL-39, R-COL-41): Windows line ends, `xml2js`, `setNextRequest` by name.
