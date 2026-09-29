# actr-mailbox

🗄️ **Actor-RTC 持久化邮箱层**

`actr-mailbox` 提供 Actor-RTC 框架的消息持久化功能，基于 SQLite 实现，确保消息的可靠存储和检索。

## 📦 功能特性

- **消息持久化**: 可靠的消息队列和邮箱存储，基于 SQLite。
- **优先级队列**: 支持高、普通两种优先级的消息。
- **ACID 保证**: 利用 SQLite 事务确保消息操作的原子性。

## 持久化恢复与独占访问

文件型 `SqliteMailbox` 使用 SQLite 的 `EXCLUSIVE` 锁模式，同一个数据库只能由一个活动实例持有。
多个任务应共享同一个 `Arc<SqliteMailbox>`，不要重复打开数据库。独占权持续到最后一个引用释放、
底层连接关闭；进程崩溃后由操作系统释放文件锁。锁冲突的 busy timeout 为 250 毫秒，
初始化失败会返回 `StorageError::ConcurrencyError`。

连接强制使用私有缓存，不受进程级共享缓存设置影响。数据库参数仅接受文件系统路径或
`:memory:`；SQLite `file:` URI 会返回 `StorageError::ConfigError`，避免 URI 参数
（如 `cache=shared`、`nolock=1` 或自定义 `vfs`）绕过独占锁。实际文件名若以 `file:` 开头，
请使用 `./file:...` 或绝对路径。

打开数据库时，实例先取得独占权，再在事务内将遗留的 `Inflight` 消息恢复为 `Queued`。
事务提交后实例才可用；恢复失败会回滚并返回错误。消息 ID、内容、优先级和创建时间不变，
已 ACK 删除的消息不会恢复。此行为适用于 WAL 和非 WAL 文件数据库；`:memory:` 实例相互独立，
关闭后不保留消息。

这提供已持久化、未 ACK 消息的至少一次投递。如果业务副作用已完成但 ACK 前崩溃，重投会重复执行，
业务需要持久化的幂等机制。运行期间，其他 SQLite 连接（包括外部只读工具）不能访问该数据库，
请通过 `Mailbox::status()` 查询状态，或在实例关闭后进行离线检查。节点的 DLQ 使用独立的 `.dlq` 数据库。

## 🚀 快速开始

在 `Cargo.toml` 中添加依赖：

```toml
[dependencies]
actr-mailbox = { path = "path/to/actr-mailbox" }
```

### 基本使用

```rust,no_run
use actr_mailbox::prelude::*;
use std::time::Duration;

async fn message_processor(mailbox: impl Mailbox) {
    loop {
        // 1. 从队列中获取一批消息
        //    此操作会自动处理优先级，并将消息标记为“处理中”
        match mailbox.dequeue().await {
            Ok(messages) => {
                if messages.is_empty() {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                }

                println!("处理 {} 条消息...", messages.len());
                for msg in messages {
                    // 2. 在这里执行你的业务逻辑
                    //    例如: process_data(msg.payload).await;
                    
                    // 3. 成功处理后，确认消息，将其从队列永久删除
                    if let Err(e) = mailbox.ack(msg.id).await {
                        eprintln!("消息 {} 确认失败: {}", msg.id, e);
                    }
                }
            }
            Err(e) => {
                eprintln!("从队列拉取消息失败: {}", e);
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        }
    }
}
```

## 🏗️ 架构层次

```text
┌─────────────────────────────────────────┐
│              actr-core                  │
├─────────────────────────────────────────┤
│  actr-sdk  │  actr-mailbox │ actr-runtime│ (此层)
├─────────────────────────────────────────┤
│  actr-protocol │    actr-version         │
├─────────────────────────────────────────┤
│  actr-types    │     actr-uri           │
└─────────────────────────────────────────┘
```

## 📄 许可证

MIT License
