#include <atomic>
#include <stdexcept>
#include <string>
#include <thread>
#include <unistd.h>

static std::atomic<int> workers{0};

static const std::string& guarded_string() {
  static const std::string value("MAKOS_LIBCXX_RUNTIME_OK\n");
  return value;
}

static void worker() {
  if (!guarded_string().empty())
    workers.fetch_add(1, std::memory_order_relaxed);
}

int main() {
  std::thread first(worker);
  std::thread second(worker);
  first.join();
  second.join();

  try {
    if (workers.load(std::memory_order_relaxed) != 2)
      throw std::runtime_error("pthread/guard failure");
  } catch (const std::exception& error) {
    const std::string message = std::string("MAKOS_LIBCXX_RUNTIME_FAIL: ") + error.what() + "\n";
    write(STDOUT_FILENO, message.data(), message.size());
    return 1;
  }

  const std::string& message = guarded_string();
  write(STDOUT_FILENO, message.data(), message.size());
  return 0;
}
