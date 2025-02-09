#define IMPORT_GDI
#include "import.hpp"

#include <gdiplus.h>

using namespace Gdiplus;

ULONG_PTR gditoken = 0;

void __declspec(dllexport) EuroScopePlugInInit(EuroScope::CPlugIn **ptr) {
	GdiplusStartupInput gdistartup;
	if (GdiplusStartup(&gditoken, &gdistartup, nullptr) != Status::Ok)
		return;
}

void __declspec(dllexport) EuroScopePlugInExit(void) {
	if (gditoken)
		GdiplusShutdown(gditoken);
}
