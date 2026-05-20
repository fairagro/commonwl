`File` contents literal.

If neither `location` nor `path` is provided, `contents` must be
non-null. The implementation must assign a unique identifier for the
`location` field. When the file is staged as input to `CommandLineTool`,
the value of `contents` must be written to a file.

If `contents` is set as a result of a Javascript expression,
an `entry` in `InitialWorkDirRequirement`, or read in from
`cwl.output.json`, there is no specified upper limit on the
size of `contents`. Implementations may have practical limits
on the size of `contents` based on memory and storage
available to the workflow runner or other factors.

If the `loadContents` field of an `InputParameter` or
`OutputParameter` is true, and the input or output `File` object
`location` is valid, the file must be a UTF-8 text file 64 KiB
or smaller, and the implementation must read the entire
contents of the file and place it in the `contents` field. If
the size of the file is greater than 64 KiB, the
implementation must raise a fatal error.
