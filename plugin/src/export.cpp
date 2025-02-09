#define IMPORT_GDI
#include "import.hpp"
#include "plugin.hpp"

#include <gdiplus.h>

using namespace Gdiplus;

Plugin *instance = nullptr;
ULONG_PTR gditoken = 0;

void __declspec(dllexport) EuroScopePlugInInit(EuroScope::CPlugIn **ptr) {
	GdiplusStartupInput gdistartup;
	if (GdiplusStartup(&gditoken, &gdistartup, nullptr) != Status::Ok)
		return;

	*ptr = instance = new Plugin;
}

void __declspec(dllexport) EuroScopePlugInExit(void) {
	if (gditoken)
		GdiplusShutdown(gditoken);

	if (instance) {
		delete instance;
		instance = nullptr;
	}
}
