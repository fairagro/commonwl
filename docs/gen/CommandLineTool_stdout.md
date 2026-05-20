Capture the command's standard output stream to a file written to
the designated output directory.

If the `CommandLineTool` contains logically chained commands
(e.g. `echo a && echo b`) `stdout` must include the output of
every command.

If `stdout` is a string, it specifies the file name to use.

If `stdout` is an expression, the expression is evaluated and must
return a string with the file name to use to capture `stdout`. If the
return value is not a string, or the resulting path contains illegal
characters (such as the path separator `/`) it is an error.
