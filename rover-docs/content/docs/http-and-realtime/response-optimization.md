---
weight: 9
title: Response Optimization
aliases:
  - /docs/server/response-optimization/
  - /docs/http-and-realtime/response-optimization/
---

Foundation includes compression negotiation and cache validator support for efficient responses.

## Compression

Rover negotiates response compression from `Accept-Encoding`.

Supported encodings:

- `gzip`
- `deflate`

Behavior notes:

- encoding selection respects quality values
- `Vary: Accept-Encoding` is maintained
- unsupported encodings are ignored

## Conditional Requests

Static responses support cache validators:

- `ETag`
- `Last-Modified`
- `Cache-Control`
- `304 Not Modified`

When client sends `If-None-Match` or `If-Modified-Since`, Rover can short-circuit body transfer.

## Compression + Caching

Validator handling stays correct when compressed and uncompressed representations coexist.

In practice:

- `ETag` remains stable enough for revalidation
- `Vary` reflects encoding negotiation
- conditional requests still work for compressed variants

## Static Asset Notes

Static assets benefit most from these semantics:

- cache headers for repeat loads
- validator-based revalidation
- safe path normalization

## Streaming Note

Long-lived chunked streams and SSE should be treated separately from cacheable asset responses. Keep proxy timeout/buffering aligned to route behavior.

## Related

- [Uploads and Static Assets](/docs/http-and-realtime/uploads-and-static-assets/)
- [Streaming](/docs/http-and-realtime/streaming/)
- [Production Deployment](/docs/operations/production-deployment/)
