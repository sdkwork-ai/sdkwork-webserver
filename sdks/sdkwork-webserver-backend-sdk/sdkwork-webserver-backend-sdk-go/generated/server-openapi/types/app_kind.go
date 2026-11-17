package types

// Business application kind: the user-facing app type of a Deploy
application. Not the identity of a carrier row — `webserver_application`
identifies itself only through `deployApplicationId` and records a
server-owned default kind.

type AppKind string
