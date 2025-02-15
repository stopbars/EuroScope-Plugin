export TARGET = i686-pc-windows-msvc
export PROFILE ?= debug

ifeq ($(PROFILE),release)
PROFILE_ARG = --release
else
ifneq ($(PROFILE),debug)
$(error Unknown compilation profile)
endif
endif

export XCC ?= clang-cl-16
export XLD ?= lld-link-16

export XWIN ?= /opt/xwin

ifeq ($(wildcard $(XWIN)/sdk/include/um/windows.h),)
$(error XWIN variable does not point to a valid installation)
endif

export EXTRALIBS = $(shell cat client/out/libs)

default: client plugin

client:
	$(MAKE) -C client/
	diff -s client/out/bars-client.hpp plugin/inc/bars-client.hpp \
		|| cp client/out/bars-client.hpp plugin/inc/
	cp target/$(TARGET)/$(PROFILE)/bars_client.lib plugin/lib/

plugin:
	$(MAKE) -C plugin/

clean:
	$(MAKE) -C plugin/ clean
	cargo clean

format:
	find plugin/ -name "*.cpp" -o -name "*.hpp" | xargs clang-format -i
	cargo fmt

.PHONY: default client plugin clean format
