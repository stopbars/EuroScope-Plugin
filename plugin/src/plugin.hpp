#pragma once

#include "import.hpp"

#include <bars-client.hpp>

class Plugin : public EuroScope::CPlugIn {
private:
	client::Context *ctx_ = nullptr;

public:
	Plugin();
	~Plugin();
	Plugin(const Plugin &) = delete;
	Plugin &operator=(const Plugin &) = delete;

	void OnTimer(int) override;

private:
	void error(const char *);
};
