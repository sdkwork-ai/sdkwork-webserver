# storage

This package owns the storage capability on the backend-admin surface. It is a thin host adapter over the drive-owned admin storage plane: the Storage Providers / Provider Catalog / Buckets / Bindings pages, their service, types, and i18n all come from `sdkwork-drive-pc-admin-storage-providers` + `sdkwork-drive-pc-commons`, which cloudrouter consumes as well. The adapter only composes the shared admin storage SDK client (`createDriveAdminStorageHostClient`), supplies the host API base URL, and adapts the host session into the drive session snapshot the shared pages expect.
