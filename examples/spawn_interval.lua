local ru = rover.ui

function rover.render()
	local ticks = rover.signal(0)
	local status = rover.signal("booting")

	local boot = rover.spawn(function()
		status.val = "running"
	end)

	local ticker = rover.interval(1000, function()
		ticks.val = ticks.val + 1
	end)

	rover.on_destroy(function()
		boot:kill()
		ticker:kill()
	end)

	return ru.column {
		ru.text { "Spawn + Interval" },
		ru.text { "boot pid: " .. boot:pid() },
		ru.text { "ticker pid: " .. ticker:pid() },
		ru.text { "status: " .. status },
		ru.text { "ticks: " .. ticks },
	}
end
