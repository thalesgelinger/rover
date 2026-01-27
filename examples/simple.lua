-- Simple test without tasks
local count = rover.signal(42)

function rover.render()
	return rover.ui.text { count }
end
