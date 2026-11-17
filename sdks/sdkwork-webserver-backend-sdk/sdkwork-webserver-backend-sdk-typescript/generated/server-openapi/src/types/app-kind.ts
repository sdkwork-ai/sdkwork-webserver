/** Business application kind: the user-facing app type of a Deploy
application. Not the identity of a carrier row — `webserver_application`
identifies itself only through `deployApplicationId` and records a
server-owned default kind.
 */
export type AppKind = 'STATIC_WEB' | 'SPA_WEB' | 'API_SERVICE' | 'WECHAT_MINIPROGRAM' | 'DOUYIN_MINIPROGRAM' | 'IOS_APP' | 'ANDROID_APP' | 'HARMONYOS_APP';
