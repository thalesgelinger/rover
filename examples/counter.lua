local ru = rover.ui

function App()
	return ru.view {
		ru.column {
			ru.text { "Hello" },
			ru.row {
				ru.text { "Rover" },
				ru.text { "Row" },
			},
			ru.text { "Column" },
		},
	}
end

return ru.render(App())
