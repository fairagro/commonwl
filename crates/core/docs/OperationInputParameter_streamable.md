Only valid when `type: File` or is an array of `items: File`.

A value of `true` indicates that the file is read or written
sequentially without seeking. An implementation may use this flag to
indicate whether it is valid to stream file contents using a named
pipe. Default: `false`.
