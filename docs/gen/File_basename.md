The base name of the file, that is, the name of the file without any
leading directory path.  The base name must not contain a slash `/`.

If not provided, the implementation must set this field based on the
`location` field by taking the final path component after parsing
`location` as an IRI.  If `basename` is provided, it is not required to
match the value from `location`.

When this file is made available to a CommandLineTool, it must be named
with `basename`, i.e. the final component of the `path` field must match
`basename`.
