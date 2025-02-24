#include "screen.hpp"

#include <cmath>
#include <format>

#include <gdiplus.h>
#include <gdiplusgraphics.h>

#define SETTING_ACTIVE "aerodrome"
#define SETTING_MENU_X "menuX"
#define SETTING_MENU_Y "menuY"

const int HEIGHT = 12;
const int PADDING = 2;

const Gdiplus::Color COLOR_MENU_DISCONNECTED(0x22, 0x22, 0x22);
const Gdiplus::Color COLOR_MENU_OBSERVING(0x1e, 0x40, 0xaf);
const Gdiplus::Color COLOR_MENU_CONTROLLING(0x16, 0x65, 0x34);
const Gdiplus::Color COLOR_MENU_FOREGROUND(0xcc, 0xcc, 0xcc);
const Gdiplus::Color COLOR_MENU_MESSAGE(0xff, 0xff, 0xff);

const Gdiplus::Point ICON_DISCONNECTED[] = {
	{4, 4}, {8, 8}, {6, 6}, {4, 8}, {8, 4}
};
const Gdiplus::Point ICON_DIRECT[] = {{6, 8}, {6, 4}, {4, 6}, {6, 4}, {8, 6}};
const Gdiplus::Point ICON_LOCAL[] = {{4, 4}, {4, 4}, {4, 4}, {4, 8}, {8, 8}};
const int ICON_N_POINTS = 5;

const int SCREEN_OBJECT_CLICK_REGION = 1;
const int SCREEN_OBJECT_MENU = 2;

const size_t AERODROME_SIZE = 4;

enum class TagFunctionType {
	None,
	OpenMenu,
	OpenEditAerodrome,
	SubmitEditAerodrome,
	ToggleControlling,
	OpenSelectProfile,
	SubmitSelectProfile,
	OpenSelectPreset,
	SubmitSelectPreset,
	OpenSelectView,
	SubmitSelectView,
};

union TagFunction {
	struct {
		TagFunctionType type : 8;
		size_t payload : 20;
	} data;
	int value;

	TagFunction(TagFunctionType type, size_t payload) : data({type, payload}) {}
	TagFunction(int type) : value(type) {}

	operator int() const { return value; }
};

Screen::Screen(client::Context *ctx, bool geo, EuroScope::CPlugIn *plugin)
	: geo_(geo), ctx_(ctx), plugin_(plugin) {
	using namespace Gdiplus;

	screen_ = client::client_create_screen(ctx, geo);

	font_family_ = new FontFamily(L"EuroScope");
	font_ = new Font(font_family_, HEIGHT, FontStyleRegular, UnitPixel);
}

Screen::~Screen() {
	delete font_;
	delete font_family_;
}

void Screen::OnAsrContentLoaded(bool loaded) {
	if (loaded) {
		const char *s;

		if ((s = GetDataFromAsr(SETTING_ACTIVE)))
			client::client_set_aerodrome(screen_, s);

		if ((s = GetDataFromAsr(SETTING_MENU_X)))
			menu_x = std::atoi(s);
		if ((s = GetDataFromAsr(SETTING_MENU_Y)))
			menu_y = std::atoi(s);
	}
}

void Screen::OnRefresh(HDC hdc, int phase) {
	using namespace Gdiplus;

	if (phase == EuroScope::REFRESH_PHASE_BACK_BITMAP) {
		const wchar_t *message = nullptr;
		if (!geo_) {
			if (!is_connected()) {
				message = L"Disconnected";
			} else if (!client::client_get_views(screen_)[0]) {
				message = L"No views defined";
			}
		}

		if (message) {
			Graphics *ctx = Graphics::FromHDC(hdc);

			SolidBrush brush(COLOR_MENU_MESSAGE);

			auto rect = GetRadarArea();

			PointF origin((rect.left + rect.right) / 2, (rect.top + rect.bottom) / 2);
			RectF text_bbox;

			ctx->MeasureString(message, -1, font_, origin, &text_bbox);

			origin.X -= text_bbox.Width / 2;

			ctx->DrawString(message, -1, font_, origin, &brush);

			delete ctx;
		} else {
			auto viewport = get_viewport();
			client::client_draw_background(screen_, hdc, viewport);
		}
	} else if (phase == EuroScope::REFRESH_PHASE_BEFORE_TAGS) {
		auto viewport = get_viewport();
		client::client_set_viewport(screen_, viewport);

		Graphics *ctx = Graphics::FromHDC(hdc);

		auto hdc2 = ctx->GetHDC();
		client::client_draw_foreground(screen_, hdc2);
		ctx->ReleaseHDC(hdc2);

		size_t n;
		const RECT *rects = client::client_get_click_regions(screen_, &n);
		for (size_t i = 0; i < n; i++) {
			AddScreenObject(SCREEN_OBJECT_CLICK_REGION, "", rects[i], false, "");
		}

		delete ctx;
	} else if (phase == EuroScope::REFRESH_PHASE_AFTER_LISTS) {
		Graphics *ctx = Graphics::FromHDC(hdc);

		Color color_menu = COLOR_MENU_DISCONNECTED;
		const Point *points = ICON_DISCONNECTED;
		const char *aerodrome = client::client_get_aerodrome(screen_);

		switch (client::client_connection_state(ctx_)) {
		case client::ConnectionState::ConnectedDirect:
		case client::ConnectionState::ConnectedProxy:
			points = ICON_DIRECT;
			break;

		case client::ConnectionState::ConnectedLocal:
			points = ICON_LOCAL;
			break;

		default:;
		}

		if (is_connected() && aerodrome) {
			switch (client::client_get_activity(screen_)) {
			case client::ActivityState::Observing:
				color_menu = COLOR_MENU_OBSERVING;
				break;

			case client::ActivityState::Controlling:
				color_menu = COLOR_MENU_CONTROLLING;
				break;

			default:;
			}
		}

		SolidBrush brush_menu(color_menu);
		SolidBrush brush_text(COLOR_MENU_FOREGROUND);
		Pen pen_icon(COLOR_MENU_FOREGROUND, 1);

		wchar_t menu_text[AERODROME_SIZE + 1] = L"BARS";
		if (aerodrome) {
			size_t i = 0;
			while (i < AERODROME_SIZE && aerodrome[i]) {
				menu_text[i] = (wchar_t)aerodrome[i];
				i++;
			}
		}

		auto rect = GetRadarArea();

		auto dx = std::min(menu_x ? menu_x : 2L, rect.right - rect.left - 40);
		auto dy = std::min(menu_y ? menu_y : 2L, rect.bottom - rect.top - 20);
		PointF origin(rect.right - dx - 2 * PADDING - HEIGHT, rect.top + dy);
		RectF text_bbox;

		ctx->MeasureString(menu_text, AERODROME_SIZE, font_, origin, &text_bbox);

		auto width = text_bbox.Width;
		origin.X -= width;

		int rect_width = width + 2 * PADDING + HEIGHT;
		int rect_height = 2 * PADDING + HEIGHT;

		ctx->FillRectangle(
			&brush_menu, (int)origin.X, (int)origin.Y, rect_width, rect_height
		);

		AddScreenObject(
			SCREEN_OBJECT_MENU, "",
			{(long)origin.X, (long)origin.Y, (long)origin.X + rect_width,
		   (long)origin.Y + rect_height},
			true, ""
		);

		origin.X += PADDING;
		origin.Y += PADDING;

		auto save = ctx->Save();
		ctx->TranslateTransform(origin.X, origin.Y);
		ctx->DrawLines(&pen_icon, points, ICON_N_POINTS);
		ctx->Restore(save);

		origin.X += HEIGHT;
		origin.Y -= PADDING;

		ctx->DrawString(menu_text, AERODROME_SIZE, font_, origin, &brush_text);

		delete ctx;

		if (client::client_is_background_refresh_required(screen_))
			RefreshMapContent();

		if (pending_function_) {
			OnFunctionCall(*pending_function_, "", {}, pending_function_area_);
			delete pending_function_;
			pending_function_ = nullptr;
		}
	}
}

void Screen::OnAsrContentToBeClosed() { delete this; }

void Screen::OnClickScreenObject(
	int type, const char *, POINT point, RECT area, int button
) {
	switch (type) {
	case SCREEN_OBJECT_CLICK_REGION:
		client::client_handle_click(
			screen_, point,
			button == EuroScope::BUTTON_LEFT ? client::ClickType::Primary
																			 : client::ClickType::Auxiliary
		);
		break;

	case SCREEN_OBJECT_MENU: {
		auto aerodrome = client::client_get_aerodrome(screen_);
		auto function = TagFunction(
			button == EuroScope::BUTTON_LEFT && aerodrome && is_connected()
				? TagFunctionType::OpenMenu
				: TagFunctionType::OpenEditAerodrome,
			0
		);
		OnFunctionCall(function, "", point, area);
		break;
	}
	}
}

void Screen::OnMoveScreenObject(
	int type, const char *, POINT point, RECT, bool release
) {
	if (type == SCREEN_OBJECT_MENU) {
		auto rect = GetRadarArea();
		int offset = PADDING + HEIGHT / 2;
		menu_x = std::max(1L, rect.right - point.x - offset);
		menu_y = std::max(1L, point.y - rect.top - offset);

		if (release) {
			auto x = std::to_string(menu_x);
			auto y = std::to_string(menu_y);

			SaveDataToAsr(SETTING_MENU_X, "Menu X position", x.c_str());
			SaveDataToAsr(SETTING_MENU_Y, "Menu Y position", y.c_str());
		}
	}
}

void Screen::OnFunctionCall(
	int type, const char *string, POINT point, RECT area
) {
	TagFunction function(type);
	switch (function.data.type) {
	case TagFunctionType::None:
		break;

	case TagFunctionType::OpenMenu: {
		plugin_->OpenPopupList(area, "BARS menu", 1);

		plugin_->AddPopupListElement(
			"Active aerodrome", "", TagFunction(TagFunctionType::OpenEditAerodrome, 0)
		);

		bool is_controller = plugin_->ControllerMyself().IsController() ||
		                     client::client_connection_state(ctx_) ==
		                       client::ConnectionState::ConnectedLocal;
		bool is_controlling = client::client_get_activity(screen_) ==
		                      client::ActivityState::Controlling;

		plugin_->AddPopupListElement(
			"Control", "",
			TagFunction(
				is_controller ? TagFunctionType::ToggleControlling
											: TagFunctionType::None,
				0
			),
			false,
			is_controlling ? EuroScope::POPUP_ELEMENT_CHECKED
										 : EuroScope::POPUP_ELEMENT_UNCHECKED,
			!is_controller
		);

		plugin_->AddPopupListElement(
			"Profiles", "", TagFunction(TagFunctionType::OpenSelectProfile, 0)
		);

		if (*client::client_get_presets(screen_))
			plugin_->AddPopupListElement(
				"Presets", "",
				TagFunction(
					is_controlling ? TagFunctionType::OpenSelectPreset
												 : TagFunctionType::None,
					0
				),
				false, EuroScope::POPUP_ELEMENT_NO_CHECKBOX, !is_controlling
			);

		if (!geo_)
			plugin_->AddPopupListElement(
				"Views", "", TagFunction(TagFunctionType::OpenSelectView, 0)
			);

		break;
	}

	case TagFunctionType::OpenEditAerodrome: {
		const char *aerodrome = client::client_get_aerodrome(screen_);
		if (!aerodrome)
			aerodrome = "";

		plugin_->OpenPopupEdit(
			area, TagFunction(TagFunctionType::SubmitEditAerodrome, 0), aerodrome
		);

		break;
	}

	case TagFunctionType::SubmitEditAerodrome:
		client::client_set_aerodrome(screen_, string[0] ? string : nullptr);
		SaveDataToAsr(SETTING_ACTIVE, "Active aerodrome", string);
		break;

	case TagFunctionType::ToggleControlling:
		client::client_set_activity(
			screen_,
			client::client_get_activity(screen_) == client::ActivityState::Observing
				? client::ActivityState::Controlling
				: client::ActivityState::Observing
		);
		break;

	case TagFunctionType::OpenSelectProfile:
		if (function.data.payload) {
			plugin_->OpenPopupList(area, "Select profile", 1);

			size_t current = client::client_get_profile(screen_);
			auto profiles = client::client_get_profiles(screen_);

			bool is_controlling = client::client_get_activity(screen_) ==
			                      client::ActivityState::Controlling;

			for (size_t i = 0; profiles[i]; i++)
				plugin_->AddPopupListElement(
					profiles[i], "",
					TagFunction(
						is_controlling ? TagFunctionType::SubmitSelectProfile
													 : TagFunctionType::None,
						i
					),
					false,
					current == i ? EuroScope::POPUP_ELEMENT_CHECKED
											 : EuroScope::POPUP_ELEMENT_UNCHECKED,
					!is_controlling
				);
		} else {
			pending_function_ =
				new TagFunction(TagFunctionType::OpenSelectProfile, 1);
			pending_function_area_ = area;
		}

		break;

	case TagFunctionType::SubmitSelectProfile:
		client::client_set_profile(screen_, function.data.payload);
		break;

	case TagFunctionType::OpenSelectPreset:
		if (function.data.payload) {
			plugin_->OpenPopupList(area, "Select preset", 1);

			auto presets = client::client_get_presets(screen_);
			for (size_t i = 0; presets[i]; i++)
				plugin_->AddPopupListElement(
					presets[i], "", TagFunction(TagFunctionType::SubmitSelectPreset, i)
				);
		} else {
			pending_function_ = new TagFunction(TagFunctionType::OpenSelectPreset, 1);
			pending_function_area_ = area;
		}

		break;

	case TagFunctionType::SubmitSelectPreset:
		client::client_apply_preset(screen_, function.data.payload);
		break;

	case TagFunctionType::OpenSelectView:
		if (function.data.payload) {
			plugin_->OpenPopupList(area, "Select view", 1);

			size_t current = client::client_get_view(screen_);
			auto views = client::client_get_views(screen_);

			for (size_t i = 0; views[i]; i++)
				plugin_->AddPopupListElement(
					views[i], "", TagFunction(TagFunctionType::SubmitSelectView, i),
					false,
					current == i ? EuroScope::POPUP_ELEMENT_CHECKED
											 : EuroScope::POPUP_ELEMENT_UNCHECKED
				);
		} else {
			pending_function_ = new TagFunction(TagFunctionType::OpenSelectView, 1);
			pending_function_area_ = area;
		}

		break;

	case TagFunctionType::SubmitSelectView:
		client::client_set_view(screen_, function.data.payload);
		break;
	}

	if (client::client_is_background_refresh_required(screen_))
		RefreshMapContent();
}

client::Viewport Screen::get_viewport() {
	client::Viewport viewport;
	auto area = GetRadarArea();

	if (geo_) {
		EuroScope::CPosition geo_origin, geo_min, geo_max, geo_lat, geo_lon;
		POINT pos_min, pos_lat, pos_lon;

		geo_origin = ConvertCoordFromPixelToPosition({0, 0});

		GetDisplayArea(&geo_min, &geo_max);
		geo_lat.m_Latitude = geo_max.m_Latitude;
		geo_lon.m_Latitude = geo_min.m_Latitude;
		geo_lat.m_Longitude = geo_min.m_Longitude;
		geo_lon.m_Longitude = geo_max.m_Longitude;

		double delta_lat = geo_max.m_Latitude - geo_min.m_Latitude;
		double delta_lon = geo_max.m_Longitude - geo_min.m_Longitude;

		pos_min = ConvertCoordFromPositionToPixel(geo_min);
		pos_lat = ConvertCoordFromPositionToPixel(geo_lat);
		pos_lon = ConvertCoordFromPositionToPixel(geo_lon);

		pos_lat.x -= pos_min.x;
		pos_lat.y -= pos_min.y;
		pos_lon.x -= pos_min.x;
		pos_lon.y -= pos_min.y;

		viewport.geo.origin[0] = geo_origin.m_Latitude;
		viewport.geo.origin[1] = geo_origin.m_Longitude;

		viewport.geo.scaling[0] = hypot(pos_lat.x, pos_lat.y) / delta_lat;
		viewport.geo.scaling[1] = hypot(pos_lon.x, pos_lon.y) / delta_lon;

		viewport.geo.rotation = atan2(pos_lon.x, pos_lon.y);

		viewport.geo.size[0] = area.right - area.left;
		viewport.geo.size[1] = area.bottom;
	} else {
		viewport.non_geo.origin[0] = viewport.non_geo.origin[1] = 0.0;

		viewport.non_geo.size[0] = area.right - area.left;
		viewport.non_geo.size[1] = area.bottom;
	}

	return viewport;
}

bool Screen::is_connected() {
	switch (client::client_connection_state(ctx_)) {
	case client::ConnectionState::Disconnected:
	case client::ConnectionState::Poisoned:
		return false;

	case client::ConnectionState::ConnectedDirect:
	case client::ConnectionState::ConnectedProxy:
	case client::ConnectionState::ConnectedLocal:
		return true;
	}
}
