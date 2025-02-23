#pragma once

#define IMPORT_GDI
#include "import.hpp"

#include <bars-client.hpp>

#include <gdiplus.h>

union TagFunction;

class Screen : public EuroScope::CRadarScreen {
private:
	bool geo_;

	client::Context *ctx_;
	client::Screen *screen_ = nullptr;

	Gdiplus::FontFamily *font_family_;
	Gdiplus::Font *font_;

	long menu_x = 0, menu_y = 0;

	TagFunction *pending_function_ = nullptr;
	RECT pending_function_area_;

	EuroScope::CPlugIn *plugin_;

public:
	Screen(client::Context *ctx, bool geo, EuroScope::CPlugIn *plugin);
	~Screen();
	Screen(const Screen &) = delete;
	Screen &operator=(const Screen &) = delete;

	void OnAsrContentLoaded(bool) override;
	void OnRefresh(HDC, int) override;
	void OnAsrContentToBeClosed() override;
	void OnClickScreenObject(int, const char *, POINT, RECT, int) override;
	void OnMoveScreenObject(int, const char *, POINT, RECT, bool) override;
	void OnFunctionCall(int, const char *, POINT, RECT) override;

private:
	client::Viewport get_viewport();
	bool is_connected();
};
