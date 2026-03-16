local ui = rover.ui
local mod = ui.mod

local count = rover.signal(0)
local card_bg = rover.signal("#dbeafe")
local text_color = rover.signal("#1e3a8a")

local function toggle_styles()
    count.val = count.val + 1

    if card_bg.val == "#dbeafe" then
        card_bg.val = "#dcfce7"
        text_color.val = "#166534"
    else
        card_bg.val = "#dbeafe"
        text_color.val = "#1e3a8a"
    end
end

function rover.render()
    return ui.column {
        mod = mod:padding(24):gap(12),
        ui.view {
            mod = mod
                :padding(16)
                :bg_color(card_bg)
                :border_width(1)
                :border_color("#93c5fd"),
            ui.text {
                mod = mod:color(text_color),
                "Styled clicks: " .. count,
            },
        },
        ui.button {
            label = "Toggle style",
            on_click = toggle_styles,
        },
    }
end
