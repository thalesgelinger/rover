package com.rovernative.roverandroid

import com.google.gson.JsonDeserializationContext
import com.google.gson.JsonDeserializer
import com.google.gson.JsonElement
import com.google.gson.annotations.SerializedName
import java.lang.reflect.Type

enum class HorizontalAlignment {
    @SerializedName("left") LEFT,
    @SerializedName("center") CENTER,
    @SerializedName("right") RIGHT
}

enum class VerticalAlignment {
    @SerializedName("top") TOP,
    @SerializedName("center") CENTER,
    @SerializedName("bottom") BOTTOM
}

sealed class Size {
    data class Value(val size: Int) : Size()
    object Full : Size()
}

class SizeDeserializer : JsonDeserializer<Size> {
    override fun deserialize(json: JsonElement, typeOfT: Type, context: JsonDeserializationContext): Size {
        return if (json.isJsonPrimitive && json.asJsonPrimitive.isString && json.asString == "full") {
            Size.Full
        } else if (json.isJsonPrimitive && json.asJsonPrimitive.isNumber) {
            Size.Value(json.asInt)
        } else {
            throw IllegalArgumentException("Unknown size value")
        }
    }
}

data class ViewProps(
    val height: Size?,
    val width: Size?,
    val horizontal: HorizontalAlignment?,
    val vertical: VerticalAlignment?,
    val color: String?
)