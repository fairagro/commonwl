Minimum reserved filesystem based storage for the designated temporary directory, in mebibytes (2\*\*20) (default is 1024)

May be a fractional value. If so, the actual storage request
must be rounded up to the next whole number. The reported
amount of storage reserved for the process, which is available
to expressions on the `CommandLineTool` as `runtime.tmpdirSize`,
must be a non-zero integer.
