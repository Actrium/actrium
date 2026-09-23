# 应用开发建议

> 非规范性文档。这里是写 Actrium 应用时的建议，不是平台规则；平台不检查，也不强制。

## 1. 平台保证什么、不保证什么

- 平台保证：对端的 Actor credential 证明它持有某把 key、拥有某个 ActorId、属于某个 Realm、声明了某个 Actor 类型。
- 平台不保证：对端实际运行的是什么代码。本机的 Actr Host 会验证自己运行的包，但远端设备归别人控制，改过的客户端也能拿到同样的 credential。
- 需要时，受管 Realm 可以要求对端出示平台远端证明（platform app attestation 等），见[运行时与协议](06-runtime-and-protocol.zh.md) §2.2。

## 2. 建议

1. **在 Service 里做授权。** 把 Front 当作普通客户端：谁能读写、能否执行某个业务命令、能否访问 Workspace，都由 Service 按 principal 与应用权限判断，不依赖界面是否隐藏了某个按钮。
2. **不信任 Guest 自报的字段。** Guest payload 中的用户、Workspace 或权限字段只是输入；以 Actr Host 附加的 session metadata 为准。
3. **不凭自报信息放权。** Front 自报的 Actor 类型、Actr Host 版本或 package digest 只用于兼容性协商，不作为授予高风险权限的依据；确实需要时，要求远端证明。

## 3. 什么时候可以放宽

Front 与 Service 在同一台设备、由同一个 Actr Host 运行（例如本机 Realm、单人使用）时，两份代码都已由这台设备验证过。此时「改过的客户端」只可能是用户自己，上述建议的收益很小。
