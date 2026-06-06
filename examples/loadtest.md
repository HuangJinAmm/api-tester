# Load Test

GET https://httpbin.org/get

- X-Test:load
- timeout:30
- retry-count:2
- retry-delay-ms:100
- retry-backoff:true

```rhai
if response.status != 200 { fail("load request failed"); }
```
