An IRI that identifies the directory resource. This may be a relative
reference, in which case it must be resolved using the base IRI of the
document. The location may refer to a local or remote resource. If
the `listing` field is not set, the implementation must use the
location IRI to retrieve directory listing. If an implementation is
unable to retrieve the directory listing stored at a remote resource (due to
unsupported protocol, access denied, or other issue) it must signal an
error.

If the `location` field is not provided, the `listing` field must be
provided. The implementation must assign a unique identifier for
the `location` field.

If the `path` field is provided but the `location` field is not, an
implementation may assign the value of the `path` field to `location`,
then follow the rules above.
