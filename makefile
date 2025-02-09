default:
	$(MAKE) -C plugin/

clean:
	$(MAKE) -C plugin/ clean

.PHONY: default clean
