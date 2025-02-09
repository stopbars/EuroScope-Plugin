ARTIFACT = bars.dll

XCC ?= clang-cl-16
XLD ?= lld-link-16

XWIN ?= /opt/xwin

ifeq ($(wildcard $(XWIN)/sdk/include/um/windows.h),)
$(error XWIN variable does not point to a valid installation)
endif

CCFLAGS = \
	--target=i686-pc-windows-msvc /std:c++20 /EHa /arch:SSE /I inc \
	/imsvc $(XWIN)/crt/include /imsvc $(XWIN)/sdk/include/shared \
	/imsvc $(XWIN)/sdk/include/ucrt /imsvc $(XWIN)/sdk/include/um \
	-Wno-microsoft
LDFLAGS = \
	/libpath:$(XWIN)/crt/lib/x86 /libpath:$(XWIN)/sdk/lib/shared/x86 \
	/libpath:$(XWIN)/sdk/lib/ucrt/x86 /libpath:$(XWIN)/sdk/lib/um/x86 \
	gdiplus.lib $(EXTRALIBS)

LIBS = $(wildcard lib/*)
SRCS = src/export.cpp
INCS = src/import.hpp $(wildcard inc/*)
OBJS = $(patsubst src/%.cpp,out/%.obj,$(SRCS))

out/$(ARTIFACT): $(OBJS) $(LIBS)
	$(XLD) /dll /out:$@ $(LDFLAGS) $^

out/%.obj: src/%.cpp $(INCS)
	$(XCC) $(CCFLAGS) /c /Fo$@ $<

clean:
	rm out/*

.PHONY: clean
