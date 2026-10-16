package io.actrium.app.data.entity

import androidx.room.ColumnInfo
import androidx.room.Entity
import androidx.room.PrimaryKey
import io.actrium.app.generated.ActriumAndroidPersistenceContract.ActiveList
import io.actrium.app.model.ActiveModel
import io.actrium.app.model.ActiveTabRowPayload
import io.actrium.app.utils.ActiveTabCoreAdapter

@Entity(tableName = ActiveList.TABLE)
data class ActiveEntity(
    @PrimaryKey
    @ColumnInfo(name = ActiveList.Column.ID)
    val id: String,
    @ColumnInfo(name = ActiveList.Column.NAME)
    val name: String,
    @ColumnInfo(name = ActiveList.Column.ICON)
    val icon: String,
    @ColumnInfo(name = ActiveList.Column.SCREENSHOT)
    val screenshot: String,
    @ColumnInfo(name = ActiveList.Column.HAS_UNREAD)
    val hasUnread: Boolean,
    @ColumnInfo(name = ActiveList.Column.URL)
    val url: String,
    @ColumnInfo(name = ActiveList.Column.IS_MARKED)
    val isMarked: Boolean,
    @ColumnInfo(name = ActiveList.Column.STATUS)
    val status: String = ActiveTabCoreAdapter.defaultStatusValue(),
    @ColumnInfo(name = ActiveList.Column.SORT_ORDER)
    val sortOrder: Int = 0
) {
    fun toActiveModel(): ActiveModel {
        return ActiveTabCoreAdapter.activeModelFromRow(toRowPayload())
    }

    private fun toRowPayload(): ActiveTabRowPayload {
        return ActiveTabRowPayload(
            id = id,
            name = name,
            icon = icon,
            screenshot = screenshot,
            hasUnread = hasUnread,
            url = url,
            isMarked = isMarked,
            status = status,
            sortOrder = sortOrder
        )
    }

    companion object {
        fun fromActiveModel(activeModel: ActiveModel, sortOrder: Int): ActiveEntity {
            return fromRowPayload(ActiveTabCoreAdapter.rowFromActiveModel(activeModel, sortOrder))
        }

        fun fromRowPayload(row: ActiveTabRowPayload): ActiveEntity {
            return ActiveEntity(
                id = row.id,
                name = row.name,
                icon = row.icon,
                screenshot = row.screenshot,
                hasUnread = row.hasUnread,
                url = row.url,
                isMarked = row.isMarked,
                status = row.status,
                sortOrder = row.sortOrder
            )
        }
    }
}
