Specifies the program to execute. If an array, the first element of
the array is the command to execute, and subsequent elements are
mandatory command line arguments. The elements in `baseCommand` must
appear before any command line bindings from `inputBinding` or
`arguments`.

If `baseCommand` is not provided or is an empty array, the first
element of the command line produced after processing `inputBinding` or
`arguments` must be used as the program to execute.

If the program includes a path separator character it must
be an absolute path, otherwise it is an error. If the program does not
include a path separator, search the `$PATH` variable in the runtime
environment of the workflow runner find the absolute path of the
executable.
