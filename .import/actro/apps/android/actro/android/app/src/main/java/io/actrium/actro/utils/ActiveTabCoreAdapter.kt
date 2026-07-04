package io.actrium.actro.utils

import io.actrium.actro.model.ActiveModel
import io.actrium.actro.model.ActiveTabRowPayload
import io.actrium.actro.model.AppStatus
import io.actrium.actro.rust.ActiveTabInfo
import io.actrium.actro.rust.ActiveTabRowInfo
import io.actrium.actro.rust.ActroCore

object ActiveTabCoreAdapter {
    fun defaultStatusValue(): String = ActroCore.defaultRuntimeAppStatus()

    fun appStatusFromCoreValue(value: String): AppStatus {
        return when (ActroCore.normalizeRuntimeAppStatus(value)) {
            ActroCore.runtimeAppStatusDead() -> AppStatus.DEAD
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
        return fromCoreInfo(ActroCore.activeTabFromRow(row.toCoreRowInfo()))
    }

    fun rowFromActiveModel(activeModel: ActiveModel, sortOrder: Int): ActiveTabRowPayload {
        return ActroCore.activeTabToRow(toCoreInfo(activeModel), sortOrder).toPayload()
    }

    fun rowsFromActiveModels(activeList: List<ActiveModel>): List<ActiveTabRowPayload> {
        return ActroCore.activeTabRowsFromTabs(activeList.map { toCoreInfo(it) })
            .map { it.toPayload() }
    }

    private fun coreStatusValue(status: AppStatus): String {
        return when (status) {
            AppStatus.ALIVE -> ActroCore.runtimeAppStatusAlive()
            AppStatus.DEAD -> ActroCore.runtimeAppStatusDead()
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
