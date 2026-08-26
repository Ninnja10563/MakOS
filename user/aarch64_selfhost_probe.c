int makos_sum3(int first, int second, int third) {
    return first + second + third;
}

int makos_adjust(int *values, int delta) {
    values[0] = values[0] + delta;
    values[1] = values[1] + delta;
    return values[0] + values[1] + values[2];
}

int makos_probe(int seed) {
    int values[3] = { seed, 18, 2 };
    if (makos_sum3(values[0], values[1], values[2]) != 40) {
        return 90;
    }
    return makos_adjust(values, 1);
}
