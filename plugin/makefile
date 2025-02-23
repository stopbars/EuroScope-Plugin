ifeq ($(MAKELEVEL),0)
$(error Run "make plugin" from the parent directory)
endif

ARTIFACT = bars.dll

CCFLAGS = \
	--target=$(TARGET) /std:c++20 /EHa /arch:SSE /I inc \
	/imsvc $(XWIN)/crt/include /imsvc $(XWIN)/sdk/include/shared \
	/imsvc $(XWIN)/sdk/include/ucrt /imsvc $(XWIN)/sdk/include/um \
	-Wno-microsoft
LDFLAGS = \
	/libpath:$(XWIN)/crt/lib/x86 /libpath:$(XWIN)/sdk/lib/shared/x86 \
	/libpath:$(XWIN)/sdk/lib/ucrt/x86 /libpath:$(XWIN)/sdk/lib/um/x86 \
	gdiplus.lib $(EXTRALIBS)

LIBS = $(wildcard lib/*)
SRCS = src/export.cpp src/plugin.cpp src/screen.cpp
INCS = src/config.hpp src/import.hpp src/plugin.hpp src/screen.hpp $(wildcard inc/*)
OBJS = $(patsubst src/%.cpp,out/%.obj,$(SRCS))

out/$(ARTIFACT): $(OBJS) $(LIBS)
	$(XLD) /dll /out:$@ $(LDFLAGS) $^

out/%.obj: src/%.cpp $(INCS)
	$(XCC) $(CCFLAGS) /c /Fo$@ $<

clean:
	rm out/*

.PHONY: clean
