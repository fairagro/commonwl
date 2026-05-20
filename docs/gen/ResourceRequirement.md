Specify basic hardware resource requirements.

"min" is the minimum amount of a resource that must be reserved to
schedule a job. If "min" cannot be satisfied, the job should not
be run.

"max" is the maximum amount of a resource that the job shall be
allocated. If a node has sufficient resources, multiple jobs may
be scheduled on a single node provided each job's "max" resource
requirements are met. If a job attempts to exceed its resource
allocation, an implementation may deny additional resources, which
may result in job failure.

If both "min" and "max" are specified, an implementation may
choose to allocate any amount between "min" and "max", with the
actual allocation provided in the `runtime` object.

If "min" is specified but "max" is not, then "max" == "min"
If "max" is specified by "min" is not, then "min" == "max".

It is an error if max < min.

It is an error if the value of any of these fields is negative.

If neither "min" nor "max" is specified for a resource, use the default values below.
