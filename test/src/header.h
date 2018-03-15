#ifndef header_h__
#define header_h__

#include <cassert>
#include <iostream>

// This counter is incremented by destructors and constructors
// and must be 0 at the end of the program
inline int &counter() {
    static int counter = 0;
    struct CheckCounter {
        ~CheckCounter() {
            assert(counter == 0);
        }
    };
    static CheckCounter checker;
    return counter;
}

// class with destructor and copy constructor
class A {
public:
  int a;
  int b;
  A(int a, int b) : a(a), b(b) { counter()++; }
  A(const A &cpy) : a(cpy.a), b(cpy.b) { counter()++; }
  ~A() { counter()--; }
  void setValues(int _a, int _b) { a = _a; b = _b; }
  int multiply() const { return a * b; }
};

// Simple struct without a destructor or copy constructor
struct B {
  int a;
  int b;
};

struct MoveOnly {
    MoveOnly(int a = 8, int b = 9) : data(a,b) { }
#if __cplusplus > 199711L
    MoveOnly(const MoveOnly &) = delete ;
    MoveOnly& operator=(const MoveOnly &) = delete ;
    MoveOnly(MoveOnly &&other) : data(other.data) { }
    MoveOnly& operator=(MoveOnly &&other) { data = other.data; return *this; }
#endif
    A data;
};


#endif // defined(header_h__)
