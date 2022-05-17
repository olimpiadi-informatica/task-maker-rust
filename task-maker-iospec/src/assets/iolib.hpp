/// Template library to read/write I/O files.
/// Do not modify.

#ifndef GENERATOR_HPP
#define GENERATOR_HPP

#include <iostream>
#include <fstream>
#include <cassert>

const bool INPUT = 0;
const bool OUTPUT = 1;

template <typename IoData>
void resize_all(IoData const &data)
{
    bool needs_space = false;
    process_io(
        const_cast<IoData &>(data),
        {},
        [](auto stream, auto &value) {},
        [](auto stream) {},
        [](auto stream, auto value) {},
        [](auto &ret, auto f, auto... args) {},
        [](auto f, auto... args) {},
        [](auto stream, auto &value, auto size)
        {
            value.resize(size);
        });
}

template <typename IoData, typename File = std::ostream>
void write_input(IoData const &data, File &file = std::cout)
{
    bool needs_space = false;
    process_io(
        const_cast<IoData &>(data),
        {},
        [&](auto stream, auto &value)
        {
            if (stream == INPUT)
            {
                if (needs_space)
                    file << " ";
                file << value;
                needs_space = true;
            }
        },
        [&](auto stream)
        {
            if (stream == INPUT)
            {
                file << std::endl;
                needs_space = false;
            }
        },
        [](auto stream, auto value) {},
        [](auto &ret, auto f, auto... args) {},
        [](auto f, auto... args) {},
        [](auto stream, auto &value, auto size)
        {
            if (stream == INPUT)
            {
                assert(value.size() == size);
            }
            value.resize(size);
        });
}

template <typename IoData, typename File = std::istream>
IoData read_input(File &file = std::cin)
{
    IoData data;

    process_io(
        data,
        {},
        [&](auto stream, auto &value)
        {
            if (stream == INPUT)
            {
                file >> value;
            }
        },
        [](auto stream) {},
        [](auto stream, auto value) {},
        [](auto &ret, auto f, auto... args) {},
        [](auto f, auto... args) {},
        [](auto stream, auto &value, auto size)
        {
            value.resize(size);
        });

    return data;
}

template <typename IoData, typename File = std::istream>
IoData run_solution(File &file = std::cin)
{
    IoData data;

    process_io(
        data,
        IoData::global_funs(),
        [&](auto stream, auto &value)
        {
            if (stream == INPUT)
            {
                file >> value;
            }
        },
        [](auto stream) {},
        [](auto stream, auto value) {},
        [](auto &ret, auto f, auto... args)
        {
            ret = f(args...);
        },
        [](auto f, auto... args)
        {
            f(args...);
        },
        [](auto stream, auto &value, auto size)
        {
            value.resize(size);
        });

    return data;
}

template <typename IoData, typename IFile = std::istream, typename OFile = std::istream>
IoData read_input_output(IFile &input_file, OFile &output_file)
{
    IoData data;

    process_io(
        data,
        {},
        [&](auto stream, auto &value)
        {
            if (stream == INPUT)
            {
                input_file >> value;
            }
            if (stream == OUTPUT)
            {
                output_file >> value;
            }
        },
        [](auto stream) {},
        [](auto stream, auto value) {},
        [](auto &ret, auto f, auto... args)
        {
            ret = f(args...);
        },
        [](auto f, auto... args)
        {
            f(args...);
        },
        [](auto stream, auto &value, auto size)
        {
            value.resize(size);
        });

    return data;
}

#define VALIDATOR_MAIN()              \
    int main(int argc, char **argv)   \
    {                                 \
        std::ifstream input(argv[1]); \
        run_solution<IoData>(input);  \
        return 0;                     \
    }

#endif
