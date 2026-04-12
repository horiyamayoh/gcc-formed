CC ?= gcc-formed
CXX ?= g++-formed
BUILD_DIR ?= $(CURDIR)/build

.PHONY: all build response-file clean

all: build response-file

build: $(BUILD_DIR)/interop_app

response-file: $(BUILD_DIR)/response.o

$(BUILD_DIR):
	mkdir -p $@

$(BUILD_DIR)/main.o: src/main.c | $(BUILD_DIR)
	$(CC) -MMD -MF $(BUILD_DIR)/main.d -c $< -o $@

$(BUILD_DIR)/helper.o: src/helper.cpp | $(BUILD_DIR)
	$(CXX) -MMD -MF $(BUILD_DIR)/helper.d -c $< -o $@

$(BUILD_DIR)/interop_app: $(BUILD_DIR)/main.o $(BUILD_DIR)/helper.o
	$(CXX) $^ -o $@

$(BUILD_DIR)/response.rsp: | $(BUILD_DIR)
	printf '%s\n' -DRESP_FROM_FILE=1 > $@

$(BUILD_DIR)/response.o: src/response.c $(BUILD_DIR)/response.rsp | $(BUILD_DIR)
	$(CC) @$(BUILD_DIR)/response.rsp -MMD -MF $(BUILD_DIR)/response.d -c $< -o $@

clean:
	rm -rf $(BUILD_DIR)
