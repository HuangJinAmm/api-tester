# Upload

POST https://httpbin.org/post

- field:description=example upload
- upload:file=./examples/login.md
- upload:meta=./README.md

```rhai
if response.status != 200 { fail("upload failed"); }
```
