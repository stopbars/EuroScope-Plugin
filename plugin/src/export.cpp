#define IMPORT_GDI
#include "import.hpp"
#include "plugin.hpp"

#include <filesystem>
#include <optional>

#include <bars-client.hpp>

#include <gdiplus.h>

using namespace Gdiplus;

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

Plugin *instance = nullptr;
ULONG_PTR gditoken = 0;

void __declspec(dllexport) EuroScopePlugInInit(EuroScope::CPlugIn **ptr) {
	GdiplusStartupInput gdistartup;
	if (GdiplusStartup(&gditoken, &gdistartup, nullptr) != Status::Ok)
		return;

	auto dir = get_dll_dir();
	if (!dir)
		return;

	client::Context *ctx = client::client_init(dir->c_str());
	if (!ctx)
		return;

	*ptr = instance = new Plugin(ctx);
}

void __declspec(dllexport) EuroScopePlugInExit(void) {
	if (gditoken)
		GdiplusShutdown(gditoken);

	if (instance) {
		delete instance;
		instance = nullptr;
	}
}
