Capture the command's standard error stream to a file written to
the designated output directory.

If `stderr` is a string, it specifies the file name to use.

If `stderr` is an expression, the expression is evaluated and must
return a string with the file name to use to capture `stderr`. If the
return value is not a string, or the resulting path contains illegal
characters (such as the path separator `/`) it is an error.
