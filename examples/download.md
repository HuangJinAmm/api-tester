# Download

GET https://httpbin.org/bytes/128

- download:./output/sample.bin

```rhai
if response.status != 200 { fail("download failed"); }
```
