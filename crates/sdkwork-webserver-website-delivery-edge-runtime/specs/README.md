# Website Delivery Edge Runtime Specs

`component.spec.json` is the machine contract for the dedicated website/Wiki delivery process.
The process consumes only the management-disabled data-plane library and owner-generated Internal
SDK adapters; it does not mount application API assemblies or open a business database.

The binary owns `serve`, `validate`, `probe`, and `relay-provider-events`. `validate` runs the same
bounded JSON Schema and semantic compiler used by `serve`, allowing deployment renderers to reject
an invalid listener policy before producing an immutable workload.
