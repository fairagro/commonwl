An IRI that identifies the file resource.  This may be a relative
reference, in which case it must be resolved using the base IRI of the
document.  The location may refer to a local or remote resource; the
implementation must use the IRI to retrieve file content.  If an
implementation is unable to retrieve the file content stored at a
remote resource (due to unsupported protocol, access denied, or other
issue) it must signal an error.

If the `location` field is not provided, the `contents` field must be
provided.  The implementation must assign a unique identifier for
the `location` field.

If the `path` field is provided but the `location` field is not, an
implementation may assign the value of the `path` field to `location`,
then follow the rules above.
