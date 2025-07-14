struct Test {
    float oui;
};

struct Test2 {
    Test test;
    float non;
};

struct Container {
    Test test;
    Test2 test2;
};

void main() {
    Container container;
    container.test.oui = 0.f;
    container.test2.non = 0.f;
}