struct Config {
    int port;
    int timeout;
};

int main() {
    Config cfg{.timeout = 30, .port = 10};
    return cfg.port;
}
