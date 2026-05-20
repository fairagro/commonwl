Minimum reserved RAM in mebibytes (2**20) (default is 256)

May be a fractional value.  If so, the actual RAM request must
be rounded up to the next whole number.  The reported amount of
RAM reserved for the process, which is available to
expressions on the CommandLineTool as `runtime.ram`, must be a
non-zero integer.
