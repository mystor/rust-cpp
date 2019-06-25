#ifndef header_h__
#define header_h__

#include <cassert>
#include <iostream>

#if __cplusplus > 199711L
#include <atomic>
typedef std::atomic<int> counter_t;
#define COUNTER_STATIC static
#else
typedef int counter_t;
#endif

// This counter is incremented by destructors and constructors
// and must be 0 at the end of the program
inline counter_t &counter() {
    static counter_t counter;
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
#if !defined (_MSC_VER) || (_MSC_VER + 0 >= 1900)
  A &operator=(const A&) = default;
#endif
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
#if !defined (_MSC_VER) || (_MSC_VER + 0 >= 1900)
    MoveOnly(const MoveOnly &) = delete ;
    MoveOnly& operator=(const MoveOnly &) = delete ;
    MoveOnly(MoveOnly &&other) : data(other.data) { }
    MoveOnly& operator=(MoveOnly &&other) { data = other.data; return *this; }
#endif
    A data;
};


#endif // defined(header_h__)
