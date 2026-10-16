package io.actrium.app.utils

import io.actrium.app.model.ActiveModel
import io.actrium.app.model.ActiveTabRowPayload
import io.actrium.app.model.AppStatus
import io.actrium.app.rust.ActiveTabInfo
import io.actrium.app.rust.ActiveTabRowInfo
import io.actrium.app.rust.ActriumCore

object ActiveTabCoreAdapter {
    fun defaultStatusValue(): String = ActriumCore.defaultRuntimeAppStatus()

    fun appStatusFromCoreValue(value: String): AppStatus {
        return when (ActriumCore.normalizeRuntimeAppStatus(value)) {
            ActriumCore.runtimeAppStatusDead() -> AppStatus.DEAD
            else -> AppStatus.ALIVE
        }
    }

    fun toCoreInfo(model: ActiveModel): ActiveTabInfo {
        return ActiveTabInfo(
            id = model.id,
            name = model.name,
            icon = model.icon,
            screenshot = model.screenshot,
            hasUnread = model.hasUnread,
            url = model.url,
            isMarked = model.isMarked,
            panelVisible = model.panelVisible,
            status = coreStatusValue(model.status),
        )
    }

    fun fromCoreInfo(info: ActiveTabInfo): ActiveModel {
        return ActiveModel(
            id = info.id,
            name = info.name,
            icon = info.icon,
            screenshot = info.screenshot,
            hasUnread = info.hasUnread,
            url = info.url,
            isMarked = info.isMarked,
            panelVisible = info.panelVisible,
            status = appStatusFromCoreValue(info.status),
        )
    }

    fun activeModelFromRow(row: ActiveTabRowPayload): ActiveModel {
        return fromCoreInfo(ActriumCore.activeTabFromRow(row.toCoreRowInfo()))
    }

    fun rowFromActiveModel(activeModel: ActiveModel, sortOrder: Int): ActiveTabRowPayload {
        return ActriumCore.activeTabToRow(toCoreInfo(activeModel), sortOrder).toPayload()
    }

    fun rowsFromActiveModels(activeList: List<ActiveModel>): List<ActiveTabRowPayload> {
        return ActriumCore.activeTabRowsFromTabs(activeList.map { toCoreInfo(it) })
            .map { it.toPayload() }
    }

    private fun coreStatusValue(status: AppStatus): String {
        return when (status) {
            AppStatus.ALIVE -> ActriumCore.runtimeAppStatusAlive()
            AppStatus.DEAD -> ActriumCore.runtimeAppStatusDead()
        }
    }

    private fun ActiveTabRowPayload.toCoreRowInfo(): ActiveTabRowInfo {
        return ActiveTabRowInfo(
            id = id,
            name = name,
            icon = icon,
            screenshot = screenshot,
            hasUnread = hasUnread,
            url = url,
            isMarked = isMarked,
            status = status,
            sortOrder = sortOrder,
        )
    }

    private fun ActiveTabRowInfo.toPayload(): ActiveTabRowPayload {
        return ActiveTabRowPayload(
            id = id,
            name = name,
            icon = icon,
            screenshot = screenshot,
            hasUnread = hasUnread,
            url = url,
            isMarked = isMarked,
            status = status,
            sortOrder = sortOrder,
        )
    }
}
