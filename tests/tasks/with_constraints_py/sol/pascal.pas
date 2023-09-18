var i : longint;
begin
  assign(input,  'input.txt');  reset(input);
  assign(output, 'output.txt'); rewrite(output);
  readln(i);
  writeln(i);
end.