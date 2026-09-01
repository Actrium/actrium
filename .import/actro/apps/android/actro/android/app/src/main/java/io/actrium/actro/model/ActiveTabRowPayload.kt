package io.actrium.actro.model

data class ActiveTabRowPayload(
    val id: String,
    val name: String,
    val icon: String,
    val screenshot: String,
    val hasUnread: Boolean,
    val url: String,
    val isMarked: Boolean,
    val status: String,
    val sortOrder: Int,
)
