/* MakOS Browser: freestanding AArch64 EL0 client.
 *
 * Real transport path: DHCP-provided DNS -> UDP DNS -> TCP -> HTTP/1.1.
 * Real document path: bounded response parse -> HTML text extraction -> wrapped
 * native-surface text. HTTPS is rejected until MakOS has certificate-verified
 * TLS; it is never silently downgraded.
 */
#include <stddef.h>
#include <stdint.h>

enum {
    SYS_WRITE = 0,
    SYS_YIELD = 1,
    SYS_CLOCK_MONOTONIC = 27,
    SYS_LOG_READ = 29,
    SYS_SURFACE_CREATE = 8,
    SYS_SURFACE_FILL = 9,
    SYS_SURFACE_PRESENT = 10,
    /* AArch64 extension range. 11/12 remain normative VFS open/read. */
    SYS_SURFACE_TEXT = 59,
    SYS_SURFACE_READ_EVENT = 60,
    SYS_SOCKET_CREATE = 47,
    SYS_SOCKET_CONNECT = 48,
    SYS_SOCKET_SEND = 49,
    SYS_SOCKET_RECEIVE = 50,
    SYS_SOCKET_CLOSE = 51,
    SYS_SURFACE_CLOSE = 58,
    SYS_NET_CONFIG = 61, /* DHCP DNS address; 52 remains package_remove. */
    SYS_SLEEP_UNTIL = 103,
};

enum { AF_INET = 2, SOCK_STREAM = 1, SOCK_DGRAM = 2 };
enum { IPPROTO_TCP = 6, IPPROTO_UDP = 17 };
enum { EVENT_KEY = 1, EVENT_POINTER = 2, EVENT_RESIZE = 3, EVENT_CLOSE = 4 };
enum { KEY_BACKSPACE = 8, KEY_ENTER = 10, KEY_UP = 0x13, KEY_DOWN = 0x14 };
enum {
    URL_CAPACITY = 512,
    REQUEST_CAPACITY = 1536,
    RESPONSE_CAPACITY = 64 * 1024,
    DOCUMENT_CAPACITY = 32 * 1024,
    HISTORY_SLOTS = 8,
};

struct surface_event {
    uint32_t kind;
    uint32_t key;
    uint32_t modifiers;
    int32_t x;
    int32_t y;
    uint32_t width;
    uint32_t height;
};

struct net_config {
    uint8_t ipv4[4];
    uint8_t gateway[4];
    uint8_t dns[4];
};

struct url {
    const char *host;
    size_t host_length;
    const char *target;
    size_t target_length;
    uint16_t port;
    uint8_t https;
};

struct document {
    char text[DOCUMENT_CAPACITY];
    size_t length;
    size_t line_offsets[1024];
    uint16_t line_lengths[1024];
    size_t line_count;
};

static uint8_t request_buffer[REQUEST_CAPACITY];
static uint8_t response_buffer[RESPONSE_CAPACITY];
static uint8_t decoded_buffer[RESPONSE_CAPACITY];
static struct document page;
static char address[URL_CAPACITY] = "http://example.com/";
static size_t address_length = sizeof("http://example.com/") - 1;
static char history[HISTORY_SLOTS][URL_CAPACITY];
static uint16_t history_lengths[HISTORY_SLOTS];
static size_t history_count;
static size_t history_cursor;
static uint64_t surface;
static uint32_t surface_width = 700;
static uint32_t surface_height = 400;
static size_t scroll_line;
static uint8_t address_selected;
static char status_text[96] = "Ready";

void *memset(void *destination, int value, size_t count) {
    uint8_t *bytes = destination;
    for (size_t index = 0; index < count; ++index)
        bytes[index] = (uint8_t)value;
    return destination;
}

void *memcpy(void *destination, const void *source, size_t count) {
    uint8_t *out = destination;
    const uint8_t *in = source;
    for (size_t index = 0; index < count; ++index)
        out[index] = in[index];
    return destination;
}

int memcmp(const void *left, const void *right, size_t count) {
    const uint8_t *first = left;
    const uint8_t *second = right;
    for (size_t index = 0; index < count; ++index)
        if (first[index] != second[index])
            return first[index] < second[index] ? -1 : 1;
    return 0;
}

static uint64_t syscall4(uint64_t number, uint64_t first, uint64_t second,
                         uint64_t third, uint64_t fourth) {
    register uint64_t x0 __asm__("x0") = first;
    register uint64_t x1 __asm__("x1") = second;
    register uint64_t x2 __asm__("x2") = third;
    register uint64_t x3 __asm__("x3") = fourth;
    register uint64_t x4 __asm__("x4") = 0;
    register uint64_t x5 __asm__("x5") = 0;
    register uint64_t x8 __asm__("x8") = number;
    __asm__ volatile("svc #0" : "+r"(x0)
                     : "r"(x1), "r"(x2), "r"(x3), "r"(x4), "r"(x5), "r"(x8)
                     : "memory", "cc");
    return x0;
}

static size_t string_length(const char *text) {
    size_t count = 0;
    while (text[count])
        ++count;
    return count;
}

static void trace(const char *text) {
    syscall4(SYS_WRITE, (uintptr_t)text, string_length(text), 0, 0);
}

static int log_read_is_denied(void) {
    uint8_t output[80];
    uint64_t metadata[3] = {
        UINT64_C(0x1122334455667788),
        UINT64_C(0x99aabbccddeeff00),
        UINT64_C(0x0123456789abcdef),
    };
    memset(output, 0xa5, sizeof(output));
    uint64_t result = syscall4(SYS_LOG_READ, 1, (uintptr_t)output,
                               sizeof(output), (uintptr_t)metadata);
    if (result != UINT64_MAX ||
        metadata[0] != UINT64_C(0x1122334455667788) ||
        metadata[1] != UINT64_C(0x99aabbccddeeff00) ||
        metadata[2] != UINT64_C(0x0123456789abcdef))
        return 0;
    for (size_t index = 0; index < sizeof(output); ++index)
        if (output[index] != 0xa5)
            return 0;
    return 1;
}

static void retry_pause(void) {
    uint64_t now = syscall4(SYS_CLOCK_MONOTONIC, 0, 0, 0, 0);
    syscall4(SYS_SLEEP_UNTIL, now + 10, 0, 0, 0);
}

static void set_status(const char *text) {
    size_t count = string_length(text);
    if (count >= sizeof(status_text))
        count = sizeof(status_text) - 1;
    memset(status_text, 0, sizeof(status_text));
    memcpy(status_text, text, count);
    trace("MAKOS_AARCH64_BROWSER_STATUS value=");
    trace(text);
    trace("\n");
}

static uint8_t lower(uint8_t byte) {
    return byte >= 'A' && byte <= 'Z' ? (uint8_t)(byte + ('a' - 'A')) : byte;
}

static int equal_case(const char *left, const char *right, size_t count) {
    for (size_t index = 0; index < count; ++index)
        if (lower((uint8_t)left[index]) != lower((uint8_t)right[index]))
            return 0;
    return 1;
}

static int parse_port(const char *value, size_t count, uint16_t *result) {
    uint32_t port = 0;
    if (!count || count > 5)
        return 0;
    for (size_t index = 0; index < count; ++index) {
        if (value[index] < '0' || value[index] > '9')
            return 0;
        port = port * 10 + (uint32_t)(value[index] - '0');
    }
    if (!port || port > 65535)
        return 0;
    *result = (uint16_t)port;
    return 1;
}

static int parse_url(const char *value, size_t count, struct url *result) {
    size_t cursor;
    memset(result, 0, sizeof(*result));
    if (count > 7 && equal_case(value, "http://", 7)) {
        cursor = 7;
        result->port = 80;
    } else if (count > 8 && equal_case(value, "https://", 8)) {
        cursor = 8;
        result->port = 443;
        result->https = 1;
    } else {
        return 0;
    }
    size_t authority_start = cursor;
    while (cursor < count && value[cursor] != '/' && value[cursor] != '?' &&
           value[cursor] != '#') {
        uint8_t byte = (uint8_t)value[cursor];
        if (byte <= 0x20 || byte >= 0x7f || byte == '@' || byte == '[' ||
            byte == ']')
            return 0;
        ++cursor;
    }
    size_t authority_end = cursor;
    size_t colon = authority_end;
    for (size_t index = authority_start; index < authority_end; ++index)
        if (value[index] == ':') {
            if (colon != authority_end)
                return 0;
            colon = index;
        }
    result->host = &value[authority_start];
    result->host_length = colon - authority_start;
    if (!result->host_length || result->host_length > 253)
        return 0;
    for (size_t index = 0; index < result->host_length; ++index) {
        uint8_t byte = (uint8_t)result->host[index];
        if (!((byte >= 'a' && byte <= 'z') || (byte >= 'A' && byte <= 'Z') ||
              (byte >= '0' && byte <= '9') || byte == '-' || byte == '.'))
            return 0;
    }
    if (colon < authority_end &&
        !parse_port(&value[colon + 1], authority_end - colon - 1, &result->port))
        return 0;
    size_t target_start = cursor;
    while (cursor < count && value[cursor] != '#') {
        uint8_t byte = (uint8_t)value[cursor];
        if (byte <= 0x20 || byte >= 0x7f)
            return 0;
        if (byte == '%') {
            if (cursor + 2 >= count)
                return 0;
            for (size_t offset = 1; offset <= 2; ++offset) {
                uint8_t digit = (uint8_t)value[cursor + offset];
                if (!((digit >= '0' && digit <= '9') ||
                      (lower(digit) >= 'a' && lower(digit) <= 'f')))
                    return 0;
            }
        }
        ++cursor;
    }
    result->target = &value[target_start];
    result->target_length = cursor - target_start;
    return 1;
}

static void socket_address(uint8_t output[8], const uint8_t ip[4], uint16_t port) {
    output[0] = AF_INET;
    output[1] = 0;
    output[2] = (uint8_t)(port >> 8);
    output[3] = (uint8_t)port;
    memcpy(&output[4], ip, 4);
}

static size_t dns_name(uint8_t *output, size_t capacity, const char *host,
                       size_t host_length) {
    size_t used = 0, start = 0;
    while (start < host_length) {
        size_t end = start;
        while (end < host_length && host[end] != '.')
            ++end;
        size_t label = end - start;
        if (!label || label > 63 || used + label + 1 >= capacity)
            return 0;
        output[used++] = (uint8_t)label;
        memcpy(&output[used], &host[start], label);
        used += label;
        start = end + 1;
    }
    output[used++] = 0;
    return used;
}

static int dns_skip_name(const uint8_t *packet, size_t count, size_t *cursor) {
    size_t labels = 0;
    while (*cursor < count && labels++ < 128) {
        uint8_t length = packet[*cursor];
        if ((length & 0xc0) == 0xc0) {
            if (*cursor + 2 > count)
                return 0;
            *cursor += 2;
            return 1;
        }
        ++*cursor;
        if (!length)
            return 1;
        if (length > 63 || *cursor + length > count)
            return 0;
        *cursor += length;
    }
    return 0;
}

static int dns_answer_ipv4(const uint8_t *packet, size_t count, uint16_t id,
                           uint8_t output[4]) {
    if (count < 12 || packet[0] != (uint8_t)(id >> 8) ||
        packet[1] != (uint8_t)id || !(packet[2] & 0x80) || (packet[3] & 0x0f))
        return 0;
    uint16_t questions = ((uint16_t)packet[4] << 8) | packet[5];
    uint16_t answers = ((uint16_t)packet[6] << 8) | packet[7];
    size_t cursor = 12;
    for (uint16_t index = 0; index < questions; ++index) {
        if (!dns_skip_name(packet, count, &cursor) || cursor + 4 > count)
            return 0;
        cursor += 4;
    }
    for (uint16_t index = 0; index < answers; ++index) {
        if (!dns_skip_name(packet, count, &cursor) || cursor + 10 > count)
            return 0;
        uint16_t type = ((uint16_t)packet[cursor] << 8) | packet[cursor + 1];
        uint16_t class_value =
            ((uint16_t)packet[cursor + 2] << 8) | packet[cursor + 3];
        uint16_t length =
            ((uint16_t)packet[cursor + 8] << 8) | packet[cursor + 9];
        cursor += 10;
        if (cursor + length > count)
            return 0;
        if (type == 1 && class_value == 1 && length == 4) {
            memcpy(output, &packet[cursor], 4);
            return 1;
        }
        cursor += length;
    }
    return 0;
}

static int parse_ipv4_literal(const char *host, size_t count, uint8_t output[4]) {
    size_t cursor = 0;
    for (size_t part = 0; part < 4; ++part) {
        uint32_t value = 0;
        size_t digits = 0;
        while (cursor < count && host[cursor] >= '0' && host[cursor] <= '9') {
            value = value * 10 + (uint32_t)(host[cursor++] - '0');
            ++digits;
        }
        if (!digits || value > 255 || (part < 3 && (cursor >= count || host[cursor++] != '.')))
            return 0;
        output[part] = (uint8_t)value;
    }
    return cursor == count;
}

static int resolve_host(const struct url *url, uint8_t output[4]) {
    if (parse_ipv4_literal(url->host, url->host_length, output))
        return 1;
    struct net_config config;
    if (syscall4(SYS_NET_CONFIG, (uintptr_t)&config, sizeof(config), 0, 0) !=
            sizeof(config) ||
        (config.dns[0] | config.dns[1] | config.dns[2] | config.dns[3]) == 0) {
        set_status("DNS config unavailable");
        return 0;
    }
    uint8_t packet[512] = {0};
    const uint16_t id = 0x4d42;
    packet[0] = (uint8_t)(id >> 8);
    packet[1] = (uint8_t)id;
    packet[2] = 1;
    packet[5] = 1;
    size_t name = dns_name(&packet[12], sizeof(packet) - 16, url->host,
                           url->host_length);
    if (!name)
        return 0;
    size_t query_length = 12 + name;
    packet[query_length + 1] = 1;
    packet[query_length + 3] = 1;
    query_length += 4;
    uint64_t socket =
        syscall4(SYS_SOCKET_CREATE, AF_INET, SOCK_DGRAM, IPPROTO_UDP, 0);
    uint8_t remote[8];
    socket_address(remote, config.dns, 53);
    if (socket == UINT64_MAX) {
        set_status("DNS socket unavailable");
        return 0;
    }
    int connected = syscall4(SYS_SOCKET_CONNECT, socket, (uintptr_t)remote,
                             sizeof(remote), 0) == 1;
    int sent = connected &&
               syscall4(SYS_SOCKET_SEND, socket, (uintptr_t)packet,
                        query_length, 0) == query_length;
    uint64_t received = 0;
    if (sent) {
        uint64_t deadline = syscall4(SYS_CLOCK_MONOTONIC, 0, 0, 0, 0) + 300;
        do {
            received = syscall4(SYS_SOCKET_RECEIVE, socket, (uintptr_t)packet,
                                sizeof(packet), 0);
            if (received && received <= sizeof(packet))
                break;
            syscall4(SYS_YIELD, 0, 0, 0, 0);
        } while (syscall4(SYS_CLOCK_MONOTONIC, 0, 0, 0, 0) < deadline);
    }
    syscall4(SYS_SOCKET_CLOSE, socket, 0, 0, 0);
    if (!connected) {
        set_status("DNS connect failed");
        return 0;
    }
    if (!sent) {
        set_status("DNS send failed");
        return 0;
    }
    if (!received || received == UINT64_MAX || received > sizeof(packet)) {
        set_status("DNS receive failed");
        return 0;
    }
    if (!dns_answer_ipv4(packet, (size_t)received, id, output)) {
        set_status("DNS response rejected");
        return 0;
    }
    return 1;
}

static size_t append(uint8_t *output, size_t capacity, size_t used,
                     const void *value, size_t count) {
    if (used + count > capacity)
        return SIZE_MAX;
    memcpy(&output[used], value, count);
    return used + count;
}

static size_t append_port(uint8_t *output, size_t capacity, size_t used,
                          uint16_t port) {
    char digits[5];
    size_t cursor = sizeof(digits);
    do {
        digits[--cursor] = (char)('0' + port % 10);
        port /= 10;
    } while (port);
    return append(output, capacity, used, &digits[cursor], sizeof(digits) - cursor);
}

static size_t make_request(const struct url *url) {
    size_t used = 0;
#define APPEND_LITERAL(value)                                                    \
    do {                                                                         \
        used = append(request_buffer, sizeof(request_buffer), used, value,       \
                      sizeof(value) - 1);                                         \
        if (used == SIZE_MAX)                                                     \
            return 0;                                                            \
    } while (0)
    APPEND_LITERAL("GET ");
    if (!url->target_length)
        APPEND_LITERAL("/");
    else if (url->target[0] == '?')
        APPEND_LITERAL("/");
    used = append(request_buffer, sizeof(request_buffer), used, url->target,
                  url->target_length);
    if (used == SIZE_MAX)
        return 0;
    APPEND_LITERAL(" HTTP/1.1\r\nHost: ");
    used = append(request_buffer, sizeof(request_buffer), used, url->host,
                  url->host_length);
    if (used == SIZE_MAX)
        return 0;
    if (url->port != 80) {
        APPEND_LITERAL(":");
        used = append_port(request_buffer, sizeof(request_buffer), used, url->port);
        if (used == SIZE_MAX)
            return 0;
    }
    APPEND_LITERAL("\r\nUser-Agent: MakOS-Browser/0.1\r\nAccept: text/html,text/plain;q=0.9,*/*;q=0.1\r\nAccept-Encoding: identity\r\nConnection: close\r\n\r\n");
#undef APPEND_LITERAL
    return used;
}

static int http_exchange(const struct url *url, size_t *response_length) {
    uint8_t ip[4];
    if (!resolve_host(url, ip)) {
        return 0;
    }
    size_t request_length = make_request(url);
    if (!request_length) {
        set_status("Request too large");
        return 0;
    }
    uint64_t socket =
        syscall4(SYS_SOCKET_CREATE, AF_INET, SOCK_STREAM, IPPROTO_TCP, 0);
    uint8_t remote[8];
    socket_address(remote, ip, url->port);
    if (socket == UINT64_MAX ||
        syscall4(SYS_SOCKET_CONNECT, socket, (uintptr_t)remote, sizeof(remote),
                 0) != 1 ||
        syscall4(SYS_SOCKET_SEND, socket, (uintptr_t)request_buffer,
                 request_length, 0) != request_length) {
        if (socket != UINT64_MAX)
            syscall4(SYS_SOCKET_CLOSE, socket, 0, 0, 0);
        set_status("Connection failed");
        return 0;
    }
    size_t used = 0;
    for (;;) {
        uint64_t count = syscall4(SYS_SOCKET_RECEIVE, socket,
                                  (uintptr_t)&response_buffer[used],
                                  sizeof(response_buffer) - used, 0);
        if (count == UINT64_MAX || count > sizeof(response_buffer) - used) {
            syscall4(SYS_SOCKET_CLOSE, socket, 0, 0, 0);
            set_status("Network receive failed");
            return 0;
        }
        if (!count)
            break;
        used += (size_t)count;
        if (used == sizeof(response_buffer)) {
            syscall4(SYS_SOCKET_CLOSE, socket, 0, 0, 0);
            set_status("Page exceeds 64 KiB limit");
            return 0;
        }
    }
    syscall4(SYS_SOCKET_CLOSE, socket, 0, 0, 0);
    *response_length = used;
    return 1;
}

static size_t find_sequence(const uint8_t *input, size_t count,
                            const char *needle, size_t needle_length) {
    if (needle_length > count)
        return SIZE_MAX;
    for (size_t index = 0; index + needle_length <= count; ++index)
        if (!__builtin_memcmp(&input[index], needle, needle_length))
            return index;
    return SIZE_MAX;
}

static int header_is(const uint8_t *line, size_t count, const char *name) {
    size_t name_length = string_length(name);
    return count > name_length && line[name_length] == ':' &&
           equal_case((const char *)line, name, name_length);
}

static int header_name_valid(const uint8_t *line, size_t colon) {
    if (!colon)
        return 0;
    for (size_t index = 0; index < colon; ++index) {
        uint8_t byte = line[index];
        if (!((byte >= 'a' && byte <= 'z') || (byte >= 'A' && byte <= 'Z') ||
              (byte >= '0' && byte <= '9') || byte == '!' || byte == '#' ||
              byte == '$' || byte == '%' || byte == '&' || byte == '\'' ||
              byte == '*' || byte == '+' || byte == '-' || byte == '.' ||
              byte == '^' || byte == '_' || byte == '`' || byte == '|' ||
              byte == '~'))
            return 0;
    }
    return 1;
}

static size_t decode_chunks(const uint8_t *input, size_t count) {
    size_t source = 0, destination = 0;
    while (source < count) {
        size_t line = find_sequence(&input[source], count - source, "\r\n", 2);
        if (line == SIZE_MAX || !line || line > 16)
            return SIZE_MAX;
        size_t chunk = 0;
        for (size_t index = 0; index < line && input[source + index] != ';'; ++index) {
            uint8_t byte = lower(input[source + index]);
            uint8_t digit;
            if (byte >= '0' && byte <= '9')
                digit = (uint8_t)(byte - '0');
            else if (byte >= 'a' && byte <= 'f')
                digit = (uint8_t)(byte - 'a' + 10);
            else
                return SIZE_MAX;
            if (chunk > (SIZE_MAX - digit) / 16)
                return SIZE_MAX;
            chunk = chunk * 16 + digit;
        }
        source += line + 2;
        if (!chunk)
            return destination;
        if (chunk > count - source || destination + chunk > sizeof(decoded_buffer) ||
            source + chunk + 2 > count || input[source + chunk] != '\r' ||
            input[source + chunk + 1] != '\n')
            return SIZE_MAX;
        memcpy(&decoded_buffer[destination], &input[source], chunk);
        destination += chunk;
        source += chunk + 2;
    }
    return SIZE_MAX;
}

static int parse_http(size_t response_length, const uint8_t **body,
                      size_t *body_length) {
    size_t end = find_sequence(response_buffer, response_length, "\r\n\r\n", 4);
    if (end == SIZE_MAX || end > 16384 || end < 12 ||
        !equal_case((const char *)response_buffer, "HTTP/1.", 7) ||
        response_buffer[8] != ' ' || response_buffer[9] < '1' ||
        response_buffer[9] > '5' || response_buffer[10] < '0' ||
        response_buffer[10] > '9' || response_buffer[11] < '0' ||
        response_buffer[11] > '9') {
        set_status("Invalid HTTP response");
        return 0;
    }
    int chunked = 0;
    size_t content_length = SIZE_MAX;
    size_t header_count = 0;
    size_t cursor = find_sequence(response_buffer, end, "\r\n", 2);
    cursor = cursor == SIZE_MAX ? end : cursor + 2;
    while (cursor < end) {
        size_t line = find_sequence(&response_buffer[cursor], end - cursor, "\r\n", 2);
        if (line == SIZE_MAX)
            line = end - cursor;
        size_t colon = find_sequence(&response_buffer[cursor], line, ":", 1);
        if (++header_count > 64 || !line || colon == SIZE_MAX ||
            !header_name_valid(&response_buffer[cursor], colon)) {
            set_status("Invalid HTTP header");
            return 0;
        }
        if (header_is(&response_buffer[cursor], line, "transfer-encoding")) {
            size_t value = string_length("transfer-encoding:");
            while (value < line && (response_buffer[cursor + value] == ' ' ||
                                    response_buffer[cursor + value] == '\t'))
                ++value;
            if (line - value != 7 ||
                !equal_case((const char *)&response_buffer[cursor + value],
                            "chunked", 7)) {
                set_status("Unsupported HTTP encoding");
                return 0;
            }
            chunked = 1;
        } else if (header_is(&response_buffer[cursor], line, "content-length")) {
            size_t value = string_length("content-length:");
            while (value < line && (response_buffer[cursor + value] == ' ' ||
                                    response_buffer[cursor + value] == '\t'))
                ++value;
            size_t parsed = 0;
            if (value == line)
                return 0;
            for (; value < line; ++value) {
                uint8_t byte = response_buffer[cursor + value];
                if (byte < '0' || byte > '9' ||
                    parsed > (SIZE_MAX - (size_t)(byte - '0')) / 10)
                    return 0;
                parsed = parsed * 10 + (size_t)(byte - '0');
            }
            if (content_length != SIZE_MAX && content_length != parsed) {
                set_status("Conflicting HTTP length");
                return 0;
            }
            content_length = parsed;
        }
        cursor += line + 2;
    }
    if (chunked && content_length != SIZE_MAX) {
        set_status("Ambiguous HTTP framing blocked");
        return 0;
    }
    const uint8_t *payload = &response_buffer[end + 4];
    size_t available = response_length - end - 4;
    if (chunked) {
        size_t decoded = decode_chunks(payload, available);
        if (decoded == SIZE_MAX) {
            set_status("Invalid chunked body");
            return 0;
        }
        *body = decoded_buffer;
        *body_length = decoded;
    } else {
        if (content_length != SIZE_MAX) {
            if (content_length > available) {
                set_status("Incomplete HTTP body");
                return 0;
            }
            available = content_length;
        }
        *body = payload;
        *body_length = available;
    }
    return 1;
}

static int tag_name_is(const uint8_t *tag, size_t count, const char *name) {
    while (count && (*tag == ' ' || *tag == '\t' || *tag == '\r' || *tag == '\n')) {
        ++tag;
        --count;
    }
    if (count && *tag == '/') {
        ++tag;
        --count;
    }
    size_t length = string_length(name);
    return count >= length && equal_case((const char *)tag, name, length) &&
           (count == length || tag[length] == ' ' || tag[length] == '\t' ||
            tag[length] == '/' || tag[length] == '\r' || tag[length] == '\n');
}

static int block_tag(const uint8_t *tag, size_t count) {
    static const char *names[] = {"p",   "div", "section", "article", "h1",
                                  "h2",  "h3",  "h4",      "li",      "br",
                                  "pre", "tr",  "blockquote"};
    for (size_t index = 0; index < sizeof(names) / sizeof(names[0]); ++index)
        if (tag_name_is(tag, count, names[index]))
            return 1;
    return 0;
}

static void document_push(uint8_t byte) {
    if (page.length + 1 < sizeof(page.text))
        page.text[page.length++] = (char)byte;
}

static void document_newline(void) {
    while (page.length && page.text[page.length - 1] == ' ')
        --page.length;
    if (page.length && page.text[page.length - 1] != '\n')
        document_push('\n');
}

static void extract_html(const uint8_t *input, size_t count) {
    memset(&page, 0, sizeof(page));
    size_t cursor = 0;
    int hidden = 0, pre = 0, pending_space = 0;
    while (cursor < count && page.length + 1 < sizeof(page.text)) {
        if (input[cursor] == '<') {
            size_t end = cursor + 1;
            uint8_t quote = 0;
            while (end < count && end - cursor <= 512) {
                uint8_t byte = input[end];
                if (!quote && (byte == '\'' || byte == '"'))
                    quote = byte;
                else if (quote == byte)
                    quote = 0;
                else if (!quote && byte == '>')
                    break;
                ++end;
            }
            if (end == count || end - cursor > 512)
                break;
            const uint8_t *tag = &input[cursor + 1];
            size_t tag_length = end - cursor - 1;
            int closing = tag_length && tag[0] == '/';
            if (tag_name_is(tag, tag_length, "script") ||
                tag_name_is(tag, tag_length, "style")) {
                if (closing) {
                    if (hidden > 0)
                        --hidden;
                } else {
                    ++hidden;
                }
            }
            else if (!hidden) {
                if (block_tag(tag, tag_length)) {
                    document_newline();
                    pending_space = 0;
                }
                if (tag_name_is(tag, tag_length, "li") && !closing) {
                    document_push('-');
                    document_push(' ');
                }
                if (tag_name_is(tag, tag_length, "pre"))
                    pre = !closing;
            }
            cursor = end + 1;
            continue;
        }
        uint8_t byte = input[cursor++];
        if (hidden)
            continue;
        if (byte == '&') {
            size_t remaining = count - cursor;
            if (remaining >= 4 && !__builtin_memcmp(&input[cursor], "amp;", 4)) {
                byte = '&';
                cursor += 4;
            } else if (remaining >= 3 && !__builtin_memcmp(&input[cursor], "lt;", 3)) {
                byte = '<';
                cursor += 3;
            } else if (remaining >= 3 && !__builtin_memcmp(&input[cursor], "gt;", 3)) {
                byte = '>';
                cursor += 3;
            } else if (remaining >= 5 && !__builtin_memcmp(&input[cursor], "nbsp;", 5)) {
                byte = ' ';
                cursor += 5;
            }
        }
        if (byte >= 0x80)
            byte = '?';
        if (pre) {
            document_push(byte == '\r' ? '\n' : byte);
        } else if (byte == ' ' || byte == '\t' || byte == '\r' || byte == '\n') {
            pending_space = page.length && page.text[page.length - 1] != '\n';
        } else {
            if (pending_space)
                document_push(' ');
            pending_space = 0;
            document_push(byte);
        }
    }
    while (page.length && (page.text[page.length - 1] == ' ' ||
                           page.text[page.length - 1] == '\n'))
        --page.length;
    page.text[page.length] = 0;
}

static void wrap_document(size_t columns) {
    page.line_count = 0;
    if (columns < 8)
        columns = 8;
    size_t start = 0;
    while (start < page.length &&
           page.line_count < sizeof(page.line_offsets) / sizeof(page.line_offsets[0])) {
        while (start < page.length && page.text[start] == '\n')
            ++start;
        if (start == page.length)
            break;
        size_t hard_end = start;
        while (hard_end < page.length && page.text[hard_end] != '\n')
            ++hard_end;
        size_t end = hard_end < start + columns ? hard_end : start + columns;
        if (end < hard_end) {
            size_t split = end;
            while (split > start && page.text[split] != ' ')
                --split;
            if (split > start)
                end = split;
        }
        page.line_offsets[page.line_count] = start;
        page.line_lengths[page.line_count] = (uint16_t)(end - start);
        ++page.line_count;
        start = end;
        while (start < hard_end && page.text[start] == ' ')
            ++start;
        if (start == hard_end && start < page.length && page.text[start] == '\n')
            ++start;
    }
}

static void fill(uint32_t color, uint32_t x, uint32_t y, uint32_t width,
                 uint32_t height) {
    uint32_t rectangle[4] = {x, y, width, height};
    syscall4(SYS_SURFACE_FILL, surface, color, (uintptr_t)rectangle, 0);
}

static void draw_text(uint32_t x, uint32_t y, const char *text, size_t count) {
    uint64_t point = ((uint64_t)x << 32) | y;
    syscall4(SYS_SURFACE_TEXT, surface, point, (uintptr_t)text, count);
}

static void render(void) {
    uint32_t content_width = surface_width > 24 ? surface_width - 24 : 1;
    uint32_t content_height = surface_height > 92 ? surface_height - 92 : 1;
    fill(0xffc0c0c0, 0, 0, surface_width, surface_height);
    fill(0xffdfdfdf, 8, 8, 44, 24);
    fill(0xffdfdfdf, 56, 8, 44, 24);
    fill(0xffffffff, 106, 8, surface_width > 118 ? surface_width - 114 : 1, 24);
    fill(0xffffffff, 8, 40, surface_width > 16 ? surface_width - 16 : 1, 22);
    fill(0xffffffff, 8, 70, content_width, content_height);
    draw_text(18, 15, "<", 1);
    draw_text(68, 15, ">", 1);
    draw_text(112, 15, address, address_length);
    draw_text(14, 47, status_text, string_length(status_text));
    size_t columns = content_width > 12 ? (content_width - 12) / 8 : 8;
    size_t visible = content_height > 12 ? (content_height - 12) / 16 : 1;
    wrap_document(columns);
    for (size_t row = 0; row < visible && scroll_line + row < page.line_count; ++row) {
        size_t offset = page.line_offsets[scroll_line + row];
        draw_text(14, 76 + (uint32_t)row * 16, &page.text[offset],
                  page.line_lengths[scroll_line + row]);
    }
    syscall4(SYS_SURFACE_PRESENT, surface, 0, 0, 0);
}

static void remember_address(void) {
    if (history_count && history_cursor < history_count &&
        history_lengths[history_cursor] == address_length &&
        !__builtin_memcmp(history[history_cursor], address, address_length))
        return;
    if (history_cursor + 1 < history_count)
        history_count = history_cursor + 1;
    if (history_count == HISTORY_SLOTS) {
        for (size_t index = 1; index < HISTORY_SLOTS; ++index) {
            memcpy(history[index - 1], history[index], URL_CAPACITY);
            history_lengths[index - 1] = history_lengths[index];
        }
        --history_count;
    }
    memcpy(history[history_count], address, address_length);
    history_lengths[history_count] = (uint16_t)address_length;
    history_cursor = history_count++;
}

static void load_current(void) {
    struct url url;
    scroll_line = 0;
    address_selected = 0;
    if (!parse_url(address, address_length, &url)) {
        set_status("Invalid URL: use http://host/path");
        const char *message = "URL rejected.";
        extract_html((const uint8_t *)message, string_length(message));
        render();
        return;
    }
    if (url.https) {
        set_status("HTTPS unavailable: verified TLS required");
        const char *message = "MakOS will not downgrade HTTPS to insecure HTTP.";
        extract_html((const uint8_t *)message, string_length(message));
        render();
        return;
    }
    set_status("Loading...");
    render();
    size_t received = 0;
    int exchanged = 0;
    for (size_t attempt = 0; attempt < 5 && !exchanged; ++attempt) {
        exchanged = http_exchange(&url, &received);
        if (!exchanged) {
            trace("MAKOS_AARCH64_BROWSER_RETRY stage=network backoff_ticks=10\n");
            retry_pause();
        }
    }
    if (!exchanged) {
        const char *message = "Network request failed.";
        extract_html((const uint8_t *)message, string_length(message));
        render();
        return;
    }
    const uint8_t *body;
    size_t body_length;
    if (!parse_http(received, &body, &body_length)) {
        const char *message = "HTTP response rejected.";
        extract_html((const uint8_t *)message, string_length(message));
        render();
        return;
    }
    extract_html(body, body_length);
    remember_address();
    set_status("Done");
    render();
    trace("MAKOS_AARCH64_BROWSER_HTTP_OK dns=1 tcp=1 http=1 parser=1 render=1\n");
}

static void restore_history(size_t index) {
    history_cursor = index;
    address_length = history_lengths[index];
    memcpy(address, history[index], address_length);
    address[address_length] = 0;
    address_selected = 0;
    load_current();
}

__attribute__((noreturn)) void _start(void) {
    trace("MAKOS_AARCH64_BROWSER_ENTRY_OK el=0 scheduled=1\n");
    if (!log_read_is_denied()) {
        trace("MAKOS_AARCH64_LOG_ACCESS_FAIL reader=browser read=unexpected\n");
        for (;;)
            __asm__ volatile("wfe");
    }
    trace("MAKOS_AARCH64_LOG_ACCESS_OK reader=browser cap_console=0 read=denied buffers=untouched\n");
    surface = syscall4(SYS_SURFACE_CREATE, surface_width, surface_height, 5, 0);
    if (!surface)
        for (;;)
            __asm__ volatile("wfe");
    const char *welcome = "MakOS Browser\nNative HTTP and readable HTML.";
    extract_html((const uint8_t *)welcome, string_length(welcome));
    render();
    trace("MAKOS_AARCH64_BROWSER_OK elf=1 surface=owned event_loop=1 native_transport=virtio-net\n");
    syscall4(SYS_SURFACE_CLOSE, surface, 0, 0, 0);
    trace("MAKOS_AARCH64_BROWSER_BACKGROUND_OK startup_fetch=0 reopen=start-menu state=retained\n");
    for (;;) {
        struct surface_event event;
        if (syscall4(SYS_SURFACE_READ_EVENT, surface, (uintptr_t)&event,
                     sizeof(event), 0) != sizeof(event)) {
            syscall4(SYS_YIELD, 0, 0, 0, 0);
            continue;
        }
        if (event.kind == EVENT_CLOSE)
            continue;
        if (event.kind == EVENT_RESIZE) {
            surface_width = event.width < 320 ? 320 : event.width;
            surface_height = event.height < 220 ? 220 : event.height;
            render();
        } else if (event.kind == EVENT_POINTER && event.key) {
            if (event.y >= 8 && event.y < 32 && event.x >= 8 && event.x < 52 &&
                history_count && history_cursor > 0)
                restore_history(history_cursor - 1);
            else if (event.y >= 8 && event.y < 32 && event.x >= 56 &&
                     event.x < 100 && history_cursor + 1 < history_count)
                restore_history(history_cursor + 1);
            else if (event.y >= 8 && event.y < 32 && event.x >= 106 &&
                     event.x < (int32_t)surface_width - 8) {
                address_selected = 1;
                set_status("Address selected; type URL and press Enter");
                render();
            }
        } else if (event.kind == EVENT_KEY) {
            uint32_t key = event.key;
            if (key == KEY_ENTER)
                load_current();
            else if (key == KEY_BACKSPACE && (address_length || address_selected)) {
                if (address_selected) {
                    address_length = 0;
                    address_selected = 0;
                } else {
                    --address_length;
                }
                address[address_length] = 0;
                render();
            } else if (key == KEY_UP && scroll_line) {
                --scroll_line;
                render();
            } else if (key == KEY_DOWN && scroll_line + 1 < page.line_count) {
                ++scroll_line;
                render();
            } else if (key >= 0x20 && key <= 0x7e) {
                if (address_selected) {
                    address_length = 0;
                    address_selected = 0;
                }
                if (address_length + 1 < sizeof(address)) {
                    address[address_length++] = (char)key;
                    address[address_length] = 0;
                    render();
                }
            }
        }
    }
}
