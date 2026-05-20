Only valid when `type: Directory` or is an array of `items: Directory`.

Specify the desired behavior for loading the `listing` field of
a `Directory` object for use by expressions.

The order of precedence for loadListing is:

1. `loadListing` on an individual parameter
1. Inherited from `LoadListingRequirement`
1. By default: `no_listing`
