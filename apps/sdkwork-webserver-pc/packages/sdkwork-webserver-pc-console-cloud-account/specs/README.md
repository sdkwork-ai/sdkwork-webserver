# cloud-account

This package owns the cloud account capability on the app-console surface. It is a thin host adapter over the IAM-owned provider account plane: the page, its controller, the scope vocabulary, and its i18n all come from `@sdkwork/iam-pc-console-cloud-account`, and the `SdkworkIamService` facade it drives is composed by `@sdkwork/webserver-pc-console-core` from the IAM app and backend clients. The adapter contributes the menu entry, the resource key, and the `manageShared` rendering decision read off the session permission scope.
