An ExpressionTool is a type of Process object that can be run by itself
or as a Workflow step. It executes a pure Javascript expression that has
access to the same input parameters as a workflow. It is meant to be used
sparingly as a way to isolate complex Javascript expressions that need to
operate on input data and produce some result; perhaps just a
rearrangement of the inputs. No Docker software container is required
or allowed.
