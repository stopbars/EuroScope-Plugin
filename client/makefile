ifeq ($(MAKELEVEL),0)
$(error Run "make client" from the parent directory)
endif

CARGOFLAGS = $(PROFILE_ARG) --target $(TARGET)

default:
	cargo build $(CARGOFLAGS)
	cargo rustc $(CARGOFLAGS) -- --print native-static-libs 2>&1 | grep -oE " \w+\.lib" > out/libs

clean:
	rm out/*

.PHONY: default clean
