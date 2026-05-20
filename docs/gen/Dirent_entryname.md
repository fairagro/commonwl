The "target" name of the file or subdirectory.  If `entry` is
a File or Directory, the `entryname` field overrides the value
of `basename` of the File or Directory object.

* Required when `entry` evaluates to file contents only
* Optional when `entry` evaluates to a File or Directory object with a `basename`
* Invalid when `entry` evaluates to an array of File or Directory objects.

If `entryname` is a relative path, it specifies a name within
the designated output directory.  A relative path starting
with `../` or that resolves to location above the designated output directory is an error.

If `entryname` is an absolute path (starts with a slash `/`)
it is an error unless the following conditions are met:

  * `DockerRequirement` is present in `requirements`
  * The program is will run inside a software container
  where, from the perspective of the program, the root
  filesystem is not shared with any other user or
  running program.

In this case, and the above conditions are met, then
`entryname` may specify the absolute path within the container
where the file or directory must be placed.
