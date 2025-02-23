#include "plugin.hpp"
#include "config.hpp"

#include <cstring>

#define COMMAND_PREFIX ".bars "
#define COMMAND_PREFIX_LEN ((size_t)6)
#define SCREEN_NAME "lighting control panel"

Plugin::Plugin(client::Context *ctx)
	: CPlugIn(
			EuroScope::COMPATIBILITY_CODE, PLUGIN_NAME, PLUGIN_VERSION,
			PLUGIN_AUTHORS, PLUGIN_LICENCE
		),
		ctx_(ctx) {
	RegisterDisplayType(SCREEN_NAME, false, false, true, true);
}

Plugin::~Plugin() { client::client_exit(ctx_); }

Screen *
Plugin::OnRadarScreenCreated(const char *name, bool, bool geo, bool, bool) {
	return geo || !std::strcmp(name, SCREEN_NAME) ? new Screen(ctx_, geo, this)
	                                              : nullptr;
}

bool Plugin::OnCompileCommand(const char *command) {
	if (std::strncmp(command, COMMAND_PREFIX, COMMAND_PREFIX_LEN))
		return false;
	command += COMMAND_PREFIX_LEN;

	if (!std::strcmp(command, "connect")) {
		switch (client::client_connection_state(ctx_)) {
		case client::ConnectionState::ConnectedDirect:
		case client::ConnectionState::ConnectedProxy:
		case client::ConnectionState::ConnectedLocal:
			client::client_disconnect(ctx_);
			break;

		default:
			switch (GetConnectionType()) {
			case EuroScope::CONNECTION_TYPE_DIRECT: {
				auto myself = ControllerMyself();
				client::client_connect_direct(
					ctx_, myself.GetCallsign(), myself.IsController()
				);
				break;
			}

			case EuroScope::CONNECTION_TYPE_VIA_PROXY:
				client::client_connect_proxy(ctx_);
				break;

			default:
				display_error("", "Not connected to network");
				break;
			}
			break;
		}
	} else if (!std::strcmp(command, "local")) {
		switch (client::client_connection_state(ctx_)) {
		case client::ConnectionState::ConnectedDirect:
		case client::ConnectionState::ConnectedProxy:
			client::client_disconnect(ctx_);

		case client::ConnectionState::Disconnected:
		case client::ConnectionState::Poisoned:
			client::client_connect_local(ctx_);
			break;

		case client::ConnectionState::ConnectedLocal:
			display_error("", "Already connected to local server");
			break;
		}
	} else {
		return false;
	}

	return true;
}

void Plugin::OnTimer(int) {
	bool connected = GetConnectionType() == EuroScope::CONNECTION_TYPE_DIRECT;
	if (connected) {
		if (client::client_connection_state(ctx_) ==
		    client::ConnectionState::Disconnected) {
			auto myself = ControllerMyself();
			client::client_connect_direct(
				ctx_, myself.GetCallsign(), myself.IsController()
			);
		}
	} else if (client::client_connection_state(ctx_) ==
	           client::ConnectionState::ConnectedDirect) {
		client::client_disconnect(ctx_);
		display_error("", "Disconnected automatically");
	}

	client::client_tick(ctx_);

	const char *message;
	while ((message = client::client_next_message(ctx_)))
		display_error("Client", message);
}

void Plugin::display_error(const char *sender, const char *message) {
	DisplayUserMessage(
		PLUGIN_NAME, sender, message, true, true, false, false, false
	);
}
