Only valid when `type: File` or is an array of `items: File`.

If true, the file (or each file in the array) must be a UTF-8
text file 64 KiB or smaller, and the implementation must read
the entire contents of the file (or file array) and place it
in the `contents` field of the File object for use by
expressions.  If the size of the file is greater than 64 KiB,
the implementation must raise a fatal error.
