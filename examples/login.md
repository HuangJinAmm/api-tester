# Login

POST https://httpbin.org/post

- Content-Type:application/json
- X-Trace:{{trace_id}}
- assert-status:200
- assert-json:json.username={{username}}

```rhai
vars["trace_id"] = uuid()
```

```json
{
  "username": "{{username}}",
  "password": "{{password}}",
  "trace": "{{trace_id}}"
}
```

```rhai
if response.status != 200 { fail("login failed"); }
```
