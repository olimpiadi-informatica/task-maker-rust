package main

import (
	"fmt"
	"io/ioutil"
	"os"
)

func main() {
	content, err := ioutil.ReadFile("input.txt")
	if err != nil {
		ioutil.WriteFile("output.txt", []byte(fmt.Sprintf("read err: %v", err)), 0644)
		os.Exit(0)
	}
	err = ioutil.WriteFile("output.txt", content, 0644)
	if err != nil {
		ioutil.WriteFile("output.txt", []byte(fmt.Sprintf("write err: %v", err)), 0644)
		os.Exit(0)
	}
}
