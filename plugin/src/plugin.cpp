#include "plugin.hpp"
#include "config.hpp"

#include <filesystem>
#include <optional>

static std::optional<std::string> get_dll_dir() {
	HMODULE module_self;
	if (!GetModuleHandleExA(
				0 | GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS |
					GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
				(LPCTSTR)get_dll_dir, &module_self
			))
		return std::string();

	char module_filename[256];
	GetModuleFileNameA(module_self, module_filename, 256);

	if (module_filename[0]) {
		std::filesystem::path path(module_filename);
		return std::optional(path.parent_path().string());
	} else {
		return std::nullopt;
	}
}

Plugin::Plugin()
	: CPlugIn(
			EuroScope::COMPATIBILITY_CODE, PLUGIN_NAME, PLUGIN_VERSION,
			PLUGIN_AUTHORS, PLUGIN_LICENCE
		) {
	auto dir = get_dll_dir();
	if (!dir) {
		error("failed to get DLL path");
		return;
	}

	if (!(ctx_ = client::client_init(dir->c_str())))
		error("initialisation failed");
}

Plugin::~Plugin() {
	if (ctx_)
		client::client_exit(ctx_);
}

void Plugin::OnTimer(int) {
	if (ctx_) {
		client::client_tick(ctx_);

		const char *message;
		while ((message = client::client_next_message(ctx_)))
			DisplayUserMessage(
				PLUGIN_NAME, "Client", message, true, true, false, false, false
			);
	}
}

void Plugin::error(const char *message) {
	DisplayUserMessage(
		PLUGIN_NAME, "", "initialisation failed", true, true, true, true, false
	);
}
