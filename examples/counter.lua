local ru = rover.ui

function ru.render()
	local value = rover.signal("Hello from reactive UI!")
	return ru.text { value }
end
