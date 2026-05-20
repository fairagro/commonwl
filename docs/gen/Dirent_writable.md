If true, the File or Directory (or array of Files or
Directories) declared in `entry` must be writable by the tool.

Changes to the file or directory must be isolated and not
visible by any other CommandLineTool process.  This may be
implemented by making a copy of the original file or
directory.

Disruptive changes to the referenced file or directory must not
be allowed unless `InplaceUpdateRequirement.inplaceUpdate` is true.

Default false (files and directories read-only by default).

A directory marked as `writable: true` implies that all files and
subdirectories are recursively writable as well.

If `writable` is false, the file may be made available using a
bind mount or file system link to avoid unnecessary copying of
the input file.  Command line tools may receive an error on
attempting to rename or delete files or directories that are
not explicitly marked as writable.
