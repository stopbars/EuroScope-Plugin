#pragma once

#include "import.hpp"
#include "screen.hpp"

#include <bars-client.hpp>

class Plugin : public EuroScope::CPlugIn {
private:
	client::Context *ctx_;

public:
	Plugin(client::Context *ctx);
	~Plugin();
	Plugin(const Plugin &) = delete;
	Plugin &operator=(const Plugin &) = delete;

	Screen *OnRadarScreenCreated(const char *, bool, bool, bool, bool) override;
	bool OnCompileCommand(const char *) override;
	void OnTimer(int) override;

private:
	void display_error(const char *sender, const char *message);
};
