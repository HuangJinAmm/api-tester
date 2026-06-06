# Cookie

GET https://httpbin.org/cookies/set/session/demo

```rhai
if response.status != 302 && response.status != 200 { fail("cookie request failed"); }
```
