# data-statistics

This package owns the data-statistics capability on the backend-admin surface: it re-exports the console capability's platform-reach Dashboard and Traffic Statistics pages and adds only the admin menu entries plus the `backend-admin` surface marker. What separates the two surfaces is reach, and reach is decided server-side: the pages here call the platform operation, which answers every tenant this edge serves and refuses a tenant-bound context outright rather than narrowing to the caller's own slice.
