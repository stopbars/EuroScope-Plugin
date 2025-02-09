default:
	$(MAKE) -C plugin/

clean:
	$(MAKE) -C plugin/ clean

format:
	find plugin/ -name "*.cpp" -o -name "*.hpp" | xargs clang-format -i
	cargo fmt

.PHONY: default clean format
