List of files or subdirectories contained in this directory.  The name
of each file or subdirectory is determined by the `basename` field of
each `File` or `Directory` object.  It is an error if a `File` shares a
`basename` with any other entry in `listing`.  If two or more
`Directory` object share the same `basename`, this must be treated as
equivalent to a single subdirectory with the listings recursively
merged.
