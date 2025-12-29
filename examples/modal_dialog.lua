local api = rover.server {}

local ModalApp = rover.component()

function ModalApp.init()
    return {
        showModal = false,
        selectedOption = "",
        formData = {
            name = "",
            email = "",
            message = ""
        },
        submitted = false
    }
end

function ModalApp.openModal(state)
    return { showModal = true, selectedOption = state.selectedOption, formData = state.formData, submitted = false }
end

function ModalApp.closeModal(state)
    return { showModal = false, selectedOption = state.selectedOption, formData = state.formData, submitted = state.submitted }
end

function ModalApp.selectOption(state, option)
    return { showModal = state.showModal, selectedOption = option, formData = state.formData, submitted = false }
end

function ModalApp.submitForm(state)
    if state.formData.name == "" or state.formData.email == "" then
        return state
    end
    
    return { showModal = false, selectedOption = state.selectedOption, formData = state.formData, submitted = true }
end

function ModalApp.clearForm(state)
    return { showModal = state.showModal, selectedOption = state.selectedOption, formData = { name = "", email = "", message = "" }, submitted = state.submitted }
end

function ModalApp.render(state)
    local data = {
        showModal = state.showModal,
        selectedOption = state.selectedOption,
        formData = state.formData,
        submitted = state.submitted
    }
    
    return rover.html(data) [=[
        <div rover-data="{ notification: false }"
             class="min-h-screen bg-gradient-to-br from-slate-900 via-purple-900 to-slate-900">
            
            <!-- Header -->
            <div class="p-8">
                <h1 class="text-4xl font-bold text-white text-center mb-2">🎨 Modal Dialog Demo</h1>
                <p class="text-purple-200 text-center">Explore different modal patterns</p>
            </div>
            
            <!-- Main Content -->
            <div class="max-w-6xl mx-auto px-4 pb-12">
                <div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-6">
                    
                    <!-- Simple Alert Modal -->
                    <div class="bg-white/10 backdrop-blur-lg rounded-2xl p-6 border border-white/20 hover:border-purple-400 transition-colors">
                        <div class="text-4xl mb-4">⚠️</div>
                        <h3 class="text-xl font-bold text-white mb-2">Alert Modal</h3>
                        <p class="text-purple-200 mb-4">Simple notification with dismiss button</p>
                        <button rover-click="openModal"
                                class="w-full bg-blue-600 hover:bg-blue-700 text-white font-medium py-2 px-4 rounded-lg transition-colors">
                            Open Alert
                        </button>
                    </div>
                    
                    <!-- Confirmation Modal -->
                    <div class="bg-white/10 backdrop-blur-lg rounded-2xl p-6 border border-white/20 hover:border-purple-400 transition-colors">
                        <div class="text-4xl mb-4">❓</div>
                        <h3 class="text-xl font-bold text-white mb-2">Confirmation</h3>
                        <p class="text-purple-200 mb-4">Ask user to confirm destructive action</p>
                        <button rover-click="openModal; selectOption('confirm')"
                                class="w-full bg-red-600 hover:bg-red-700 text-white font-medium py-2 px-4 rounded-lg transition-colors">
                            Open Confirmation
                        </button>
                    </div>
                    
                    <!-- Form Modal -->
                    <div class="bg-white/10 backdrop-blur-lg rounded-2xl p-6 border border-white/20 hover:border-purple-400 transition-colors">
                        <div class="text-4xl mb-4">📝</div>
                        <h3 class="text-xl font-bold text-white mb-2">Form Modal</h3>
                        <p class="text-purple-200 mb-4">Collect user input with validation</p>
                        <button rover-click="openModal; selectOption('form')"
                                class="w-full bg-green-600 hover:bg-green-700 text-white font-medium py-2 px-4 rounded-lg transition-colors">
                            Open Form
                        </button>
                    </div>
                </div>
                
                <!-- Success Message -->
                <div x-show="submitted" 
                     x-transition:enter="transition ease-out duration-300"
                     x-transition:enter-start="opacity-0 translate-y-4"
                     x-transition:enter-end="opacity-100 translate-y-0"
                     class="mt-8 bg-green-500 text-white rounded-xl p-4 text-center font-medium">
                    ✅ Form submitted successfully!
                </div>
            </div>
            
            <!-- Modal Overlay -->
            <div x-show="showModal"
                 x-transition:enter="transition ease-out duration-300"
                 x-transition:enter-start="opacity-0"
                 x-transition:enter-end="opacity-100"
                 x-transition:leave="transition ease-in duration-200"
                 x-transition:leave-start="opacity-100"
                 x-transition:leave-end="opacity-0"
                 @click="if ($event.target === $el) closeModal()"
                 class="fixed inset-0 bg-black/50 backdrop-blur-sm z-50 flex items-center justify-center p-4">
                
                <!-- Alert Modal Content -->
                <div x-show="selectedOption === ''"
                     x-transition:enter="transition ease-out duration-300"
                     x-transition:enter-start="opacity-0 scale-95"
                     x-transition:enter-end="opacity-100 scale-100"
                     x-transition:leave="transition ease-in duration-200"
                     x-transition:leave-start="opacity-100 scale-100"
                     x-transition:leave-end="opacity-0 scale-95"
                     class="bg-white rounded-2xl shadow-2xl max-w-md w-full p-6">
                    <div class="text-5xl mb-4">⚠️</div>
                    <h2 class="text-2xl font-bold mb-2">Alert!</h2>
                    <p class="text-gray-600 mb-6">This is a simple alert modal. Click the button below to dismiss it.</p>
                    <div class="flex justify-end">
                        <button rover-click="closeModal"
                                class="bg-blue-600 hover:bg-blue-700 text-white font-medium py-2 px-6 rounded-lg transition-colors">
                            Got it!
                        </button>
                    </div>
                </div>
                
                <!-- Confirmation Modal Content -->
                <div x-show="selectedOption === 'confirm'"
                     x-transition:enter="transition ease-out duration-300"
                     x-transition:enter-start="opacity-0 scale-95"
                     x-transition:enter-end="opacity-100 scale-100"
                     x-transition:leave="transition ease-in duration-200"
                     x-transition:leave-start="opacity-100 scale-100"
                     x-transition:leave-end="opacity-0 scale-95"
                     class="bg-white rounded-2xl shadow-2xl max-w-md w-full p-6">
                    <div class="text-5xl mb-4">🗑️</div>
                    <h2 class="text-2xl font-bold mb-2">Delete Item?</h2>
                    <p class="text-gray-600 mb-6">This action cannot be undone. Are you sure you want to delete this item?</p>
                    <div class="flex gap-3 justify-end">
                        <button rover-click="closeModal"
                                class="bg-gray-200 hover:bg-gray-300 text-gray-800 font-medium py-2 px-6 rounded-lg transition-colors">
                            Cancel
                        </button>
                        <button rover-click="closeModal"
                                class="bg-red-600 hover:bg-red-700 text-white font-medium py-2 px-6 rounded-lg transition-colors">
                            Delete
                        </button>
                    </div>
                </div>
                
                <!-- Form Modal Content -->
                <div x-show="selectedOption === 'form'"
                     x-transition:enter="transition ease-out duration-300"
                     x-transition:enter-start="opacity-0 scale-95"
                     x-transition:enter-end="opacity-100 scale-100"
                     x-transition:leave="transition ease-in duration-200"
                     x-transition:leave-start="opacity-100 scale-100"
                     x-transition:leave-end="opacity-0 scale-95"
                     class="bg-white rounded-2xl shadow-2xl max-w-md w-full p-6">
                    <div class="text-5xl mb-4">📝</div>
                    <h2 class="text-2xl font-bold mb-4">Contact Us</h2>
                    
                    <div class="space-y-4">
                        <div>
                            <label class="block text-sm font-medium text-gray-700 mb-1">Name</label>
                            <input type="text"
                                   rover-model="formData.name"
                                   placeholder="Your name"
                                   class="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-purple-500 focus:border-transparent" />
                        </div>
                        
                        <div>
                            <label class="block text-sm font-medium text-gray-700 mb-1">Email</label>
                            <input type="email"
                                   rover-model="formData.email"
                                   placeholder="your@email.com"
                                   class="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-purple-500 focus:border-transparent" />
                        </div>
                        
                        <div>
                            <label class="block text-sm font-medium text-gray-700 mb-1">Message</label>
                            <textarea rover-model="formData.message"
                                      rows="3"
                                      placeholder="Your message..."
                                      class="w-full px-3 py-2 border border-gray-300 rounded-lg focus:outline-none focus:ring-2 focus:ring-purple-500 focus:border-transparent resize-none"></textarea>
                        </div>
                    </div>
                    
                    <div class="flex gap-3 justify-end mt-6">
                        <button rover-click="closeModal"
                                class="bg-gray-200 hover:bg-gray-300 text-gray-800 font-medium py-2 px-6 rounded-lg transition-colors">
                            Cancel
                        </button>
                        <button rover-click="submitForm"
                                class="bg-green-600 hover:bg-green-700 text-white font-medium py-2 px-6 rounded-lg transition-colors">
                            Submit
                        </button>
                    </div>
                </div>
            </div>
        </div>
    ]=]
end

function api.get()
    local data = { ModalApp = ModalApp }
    return api.html(data) [=[
        <!DOCTYPE html>
        <html>
        <head>
            <title>Modal Dialog Demo</title>
            <script src="https://cdn.tailwindcss.com"></script>
        </head>
        <body class="antialiased">
            {{ ModalApp() }}
        </body>
        </html>
    ]=]
end

return api
