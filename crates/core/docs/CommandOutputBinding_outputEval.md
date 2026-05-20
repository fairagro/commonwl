Evaluate an expression to generate the output value. If
`glob` was specified, the value of `self` must be an array
containing file objects that were matched. If no files were
matched, `self` must be a zero length array; if a single file
was matched, the value of `self` is an array of a single
element. The exit code of the process is
available in the expression as `runtime.exitCode`.

Additionally, if `loadContents` is true, the file must be a
UTF-8 text file 64 KiB or smaller, and the implementation must
read the entire contents of the file (or file array) and place
it in the `contents` field of the `File` object for use in
`outputEval`. If the size of the file is greater than 64 KiB,
the implementation must raise a fatal error.

If a tool needs to return a large amount of structured data to
the workflow, loading the output object from `cwl.output.json`
bypasses `outputEval` and is not subject to the 64 KiB
`loadContents` limit.
