#include "plugin.hpp"
#include "config.hpp"

#include <bars-client.hpp>

Plugin::Plugin()
	: CPlugIn(
			EuroScope::COMPATIBILITY_CODE, PLUGIN_NAME, PLUGIN_VERSION,
			PLUGIN_AUTHORS, PLUGIN_LICENCE
		) {
	DisplayUserMessage(
		PLUGIN_NAME, "", (const char *)hello(), true, true, true, true, false
	);
}
