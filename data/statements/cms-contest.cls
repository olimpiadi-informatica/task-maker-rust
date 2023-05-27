% cms-contest
%
% Copyright (c) 2015 Gabriele Farina (gabr.farina@gmail.com)
%
% Licensed under the Apache License, Version 2.0 (the "License");
% you may not use this file except in compliance with the License.
% You may obtain a copy of the License at
%
%    http://www.apache.org/licenses/LICENSE-2.0
%
% Unless required by applicable law or agreed to in writing, software
% distributed under the License is distributed on an "AS IS" BASIS,
% WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
% See the License for the specific language governing permissions and
% limitations under the License.
%
% Based on the work at
%  - https://github.com/GassaFM/olymp.sty/
%  - https://github.com/boi-2014/tasks/


\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{cms-contest}
\LoadClass[11pt,a4paper]{article}

\RequirePackage{enumitem}
\RequirePackage{graphicx}
\RequirePackage{amsmath}
\RequirePackage{amssymb}
\RequirePackage{color} % for colored text
\RequirePackage{comment}
\RequirePackage{import} % for changing current dir
\RequirePackage{epigraph}
\RequirePackage{wrapfig} % for having text alongside pictures
\RequirePackage{verbatim}
\RequirePackage{tikz}
\RequirePackage{booktabs}
\RequirePackage{eurosym}
\RequirePackage{fancyhdr}
\RequirePackage{calc}
\RequirePackage{float}
\RequirePackage{lastpage}
\RequirePackage{listings}
\RequirePackage{lmodern}
\RequirePackage{multirow}
\RequirePackage{placeins}
\RequirePackage[compact]{titlesec}
\RequirePackage{ifthen}
\RequirePackage{tabularx}
\RequirePackage{environ}
\RequirePackage{fontawesome}
\RequirePackage{siunitx}
\RequirePackage{currfile}
\RequirePackage{xstring}
\RequirePackage{catchfile}
\RequirePackage{booktabs}


\pagestyle{fancy}

\newif\if@showproblemletter
\newif\if@showproblemnumber
\newif\if@intentionallyblankpages

\newif\if@showlogo\@showlogofalse
\newif\if@english\@englishtrue
\newif\if@italian
\newif\if@spanish
\newif\if@showsolutions\@showsolutionsfalse
\newif\if@showsummary\@showsummaryfalse

\DeclareOption{problemletters}{\@showproblemlettertrue}
\DeclareOption{problemnumbers}{\@showproblemnumbertrue}
\DeclareOption{intentionallyblankpages}{\@intentionallyblankpagestrue}
\DeclareOption{showsolutions}{\@showsolutionstrue}
\DeclareOption{showsummary}{\@showsummarytrue}

% Language support
\gdef\@languagecode{EN}
\DeclareOption{english}{}
\DeclareOption{italian}{\@englishfalse\@italiantrue\gdef\@languagecode{IT}}
\DeclareOption{spanish}{\@englishfalse\@spanishtrue\gdef\@languagecode{ES}}

\ProcessOptions\relax

%*********************************************************************
%                        Import language file                        *
%*********************************************************************

\if@italian \input{locale/italian}\fi
\if@spanish \input{locale/spanish}\fi
\if@english \input{locale/english}\fi

%*********************************************************************
%                          Sizes and margins                         *
%*********************************************************************

\newlength{\thelinewidth}
\newlength{\exmpwidinf}
\newlength{\exmpwidouf}
\newlength{\exmpwidewid}
\newlength{\exmpthreewidinf}
\newlength{\exmpthreewidouf}
\newlength{\exmpthreewidnote}
\newlength{\afterproblemhead}
\newlength{\afterconstraints}
\newlength{\logolength}
\newlength{\logomargin}

\renewcommand{\headrulewidth}{0pt}
\renewcommand{\footrulewidth}{0pt}

\headheight=0mm
\headsep=8mm
\parindent=0mm
\parskip=6pt
\hoffset=-24mm
\voffset=-20mm
\textheight=258mm
\textwidth=175mm

\exmpwidinf=0.475\textwidth
\exmpwidouf=0.475\textwidth
\exmpwidewid=0.9\textwidth
\exmpthreewidinf=0.28\textwidth
\exmpthreewidouf=0.28\textwidth
\exmpthreewidnote=0.30\textwidth
\afterproblemhead=3mm
\afterconstraints=2mm
\logolength=21mm
\logomargin=3mm

%*********************************************************************
%                           Internal variables                       *
%*********************************************************************
\gdef\this@contestname{Contest name}
\gdef\this@contestlocation{Contest location}
\gdef\this@contestdate{Contest date}
\gdef\this@contestday{}
\gdef\this@contestlogo{logo.png}

\gdef\this@problemtitle{Problem Title}
\gdef\last@problemtitle{}
\gdef\this@problemshorttitle{Problem Short Title}
\gdef\last@problemshorttitle{}
\gdef\this@problemauthor{}
\gdef\last@problemauthor{}
\gdef\this@inputfilename{}
\gdef\this@outputfilename{}
\gdef\this@problemrevnumber{}

%*********************************************************************
%                           Internal macros                          *
%*********************************************************************


\lhead{}\rhead{}
\chead{}
\lfoot{}\rfoot{}
\cfoot{\small
    \ifthenelse{\equal{\last@problemauthor}{}}%
    {%
        \gdef\footercontent{%
            \if@showproblemletter\kw@Problem~\Alph{@problemcounter}.\ \fi%
            \if@showproblemnumber\kw@Problem~\arabic{@problemcounter}.\ \fi%
            ~\texttt{\this@problemshorttitle\ifthenelse{\equal{\this@problemrevnumber}{}}{}{\enskip\textemdash\enskip\kw@revision \this@problemrevnumber}}%
            {\hfill}%
            \kw@Page\ \thepage\ \kw@of\ \pageref{LastPage}\hfill%
        }%
    }%
    {%
        \gdef\footercontent{%
            \if@showproblemletter\kw@Problem~\Alph{@problemcounter}.\ \fi%
            \if@showproblemnumber\kw@Problem~\arabic{@problemcounter}.\ \fi%
            ~\texttt{\this@problemshorttitle\ifthenelse{\equal{\this@problemrevnumber}{}}{}{\enskip$\bullet$\enskip\kw@revision \this@problemrevnumber}}%
            {\hfill}%
            {\kw@ProblemAuthor~\last@problemauthor}%
            {\hfill}%
            {\kw@Page\ \thepage\ \kw@of\ \pageref{LastPage}}%
            \quad~%
        }%
    }%
	%
    \parbox{\textwidth}{ %
        \hrule %
        \vspace{6pt} %
        \sffamily %
        \footercontent %
    } %
}

% References to "lastpage"
\def\lastpage@putlabel{\addtocounter{page}{-1}%
   \immediate\write\@auxout{\string\newlabel{LastPage}{{}{\thepage}}}%
   \addtocounter{page}{1}}
\AtEndDocument{\clearpage\lastpage@putlabel}%


% Delete space after verbatiminput
\addto@hook{\every@verbatim}{\topsep=0pt\relax}

%*********************************************************************
%                           Internal values                          *
%*********************************************************************

% @problemcounter
%
% knows the number of the current problem
\newcounter{@problemcounter}

% @problemtaskcounter
%
% knows the number of the current subtask (relative to the current problem)
\newcounter{@subtaskcounter}[@problemcounter]

% @oisubtaskcounter -- counter for OIS style subtasks
%
% knows the number of the current subtask (relative to the current problem)
\newcounter{@oisubtaskcounter}[@problemcounter]


%*********************************************************************
%                         Internal commands                          *
%*********************************************************************

% @newsection
%
% This acts like a decorator, and registers a section command such as
% the "Specifications", "Constraints", "Subtask 1" sections
%
% Arguments: 1) new command name     e.g. \Examples
%            2) displayed title      e.g. Sample Cases
\def\@newsection#1#2{
    \DeclareRobustCommand{#1}{
        {
            \beforeproblemsectioncaption
            \noindent\bf\problemsectionfont\textsf{#2}
        }
        \nopagebreak
        \par\afterproblemsectioncaption
    }
}

% @newsectionwithdescr
%
% This acts like a decorator, and registers a section command such as
% the "Specifications", "Constraints", "Subtask 1" sections
%
% Arguments: 1) new command name     e.g. \Examples
%            2) displayed title      e.g. Sample Cases
%
% The resulting command will accept one argument, which will be appended
% to the title.
\def\@newsectionwithdescr#1#2{
    \DeclareRobustCommand{#1}[1]{
        {
            \beforeproblemsectioncaption
            \noindent\bf\problemsectionfont\textsf{#2~##1}
        }
        \nopagebreak
        \par\afterproblemsectioncaption
    }
}


% @problemend
%
% this is run as the final action in the problem environment and must
% account for the intentionally blank pages when the option is set
\newcommand{\@problemend}{%
    \clearpage%
    \if@intentionallyblankpages%
        \ifodd\value{page}%
        \else%
            \vspace*{\fill}%
            \begin{center}%
                \problemheadfont%
                \begin{tikzpicture}%
                    \draw (0,0) node[rectangle, draw=black!40, line width=1.5mm, inner sep = 1cm]%
                        {\textcolor{black!60}{\bfseries \kw@IntentionallyBlankPage}};%
                \end{tikzpicture}%
            \end{center}%
            \vspace*{\fill}%
            \clearpage%
        \fi%
    \fi%
}

%*********************************************************************
%                           Public commands                          *
%*********************************************************************

% problemheadfont, problemsectionfont, problemtextfont
%
% define standard fonts for differen parts of the problem env.
\newcommand{\problemheadfont}{\LARGE}
\newcommand{\problemsectionfont}{\Large}
\newcommand{\problemtextfont}{\normalsize}

% beforeproblemsectioncaption, afterproblemsectioncaption
\newcommand{\beforeproblemsectioncaption}{\smallbreak\bigskip}
\newcommand{\afterproblemsectioncaption}{\smallskip}

% Expose the newsection* decorators
\newcommand{\createsection}{\@newsection}
\newcommand{\createsectionwithdescr}{\@newsectionwithdescr}

% Define standard sections
\createsection{\Specification}{\kw@Specification}
\createsection{\InputFile}{\kw@Input}
\createsection{\OutputFile}{\kw@Output}
\createsection{\Example}{\kw@Example}
\createsection{\Examples}{\kw@Examples}
\createsection{\Explanation}{\kw@Explanation}
\createsection{\Explanations}{\kw@Explanations}
\createsection{\Illustration}{\kw@Illustration}
\createsection{\Implementation}{\kw@Implementation}
\createsection{\Scoring}{\kw@Scoring}
\createsection{\Note}{\kw@Note}
\createsection{\Notes}{\kw@Notes}
\createsection{\Constraints}{\kw@Constraints}
\createsection{\SubtaskOne}{\kw@SubtaskOne}
\createsection{\SubtaskTwo}{\kw@SubtaskTwo}
\createsection{\SubtaskThree}{\kw@SubtaskThree}
\createsection{\SubtaskFour}{\kw@SubtaskFour}
\createsection{\SubtaskFive}{\kw@SubtaskFive}
\createsection{\SubtaskSix}{\kw@SubtaskSix}
\createsectionwithdescr{\Subtask}{\kw@Subtask}
\createsection{\Solution}{\kw@Solution}
\createsection{\SampleGrader}{\kw@SampleGrader}

% SubtaskWithScore
%
% creates the section title for a subtask, specifying the number
% of points. This automatically increments the subtask counter.
%
% Arguments: 1) score for the current subtask   e.g. 20

\newcommand{\SubtaskWithScore}[1]{
    {
        \addtocounter{@subtaskcounter}{1}
        \beforeproblemsectioncaption

        \noindent\problemsectionfont\textsf{\textbf{%
            \kw@Subtask~\arabic{@subtaskcounter}%
        }}%
        \textsf{~(\kw@points:~#1)}

        \nopagebreak
        \par\afterproblemsectioncaption
    }
}


% Level
%
% prints a description of the knowledge level required by the solution, on a scale
% from 0 to 5, as a number of books, e.g. \Level{3} will print 3 books out of 5
%
% Arguments: 1) number of books, min 0 max 5   e.g. 3

\newcommand{\Level}[1]{
	\ifnum#1=0\textcolor{orange}{}\textcolor{gray}{\faBook\faBook\faBook\faBook\faBook}\fi
	\ifnum#1=1\textcolor{orange}{\faBook}\textcolor{gray}{\faBook\faBook\faBook\faBook}\fi
	\ifnum#1=2\textcolor{orange}{\faBook\faBook}\textcolor{gray}{\faBook\faBook\faBook}\fi
	\ifnum#1=3\textcolor{orange}{\faBook\faBook\faBook}\textcolor{gray}{\faBook\faBook}\fi
	\ifnum#1=4\textcolor{orange}{\faBook\faBook\faBook\faBook}\textcolor{gray}{\faBook}\fi
	\ifnum#1=5\textcolor{orange}{\faBook\faBook\faBook\faBook\faBook}\textcolor{gray}{}\fi
}

% OISubtask  -- subtask in OIS style: www.oisquadre.it
%
% creates the section title for a subtask, specifying the number
% of points and the level. This automatically increments the subtask counter.
%
% Arguments: 1) score for the current subtask   e.g. 20
% Arguments: 2) level for the current subtask   e.g. 3

\newcommand{\OISubtask}[4]{
  {
    \addtocounter{@oisubtaskcounter}{1}

    \begin{minipage}[t]{0.3\textwidth}
  	\textendash{}~\textbf{\kw@Subtask~\arabic{@oisubtaskcounter}}~(#1~\kw@points)\\
  	\phantom{\textendash{}}~\Level{#2}
    \end{minipage}
    \begin{minipage}[t]{0.7\textwidth}
  	#3
    \end{minipage}
  }
}

\newcommand{\setContestDay}[1]{
	\gdef\this@contestday{#1}
}

\newcommand{\setProblemAuthor}[1]{
	\gdef\this@problemauthor{#1}
}

\newcommand{\setProblemRevisionNumber}[1]{
	\gdef\this@problemrevnumber{#1}
}

\newcommand{\setContestLogo}[1]{
    \ifthenelse{\equal{#1}{}}{}{%
	   \@showlogotrue%
	   \gdef\this@contestlogo{#1}%
    }
}


% Tools to extract subtask and task limits and scores.
\newwrite\py@num@tmp
\newcommand{\pynum}[2][]{%
\immediate\openout\py@num@tmp=\currfiledir/py_num.py%
\if\relax\detokenize{#1}\relax
\else
\immediate\write\py@num@tmp{from #1 import *}%
\fi
\immediate\write\py@num@tmp{print(#2)}%
\immediate\closeout\py@num@tmp%
\immediate\write18{cd \currfiledir; python3 py_num.py > ../py_num.tex}%
\ifnum0\pdffilesize{py_num.tex}>0
\CatchFileDef{\py@num}{py_num.tex}{}%
\IfDecimal{\py@num}{\num{\py@num}}{\PackageError{pynum}{Python output for #2 is not a number}{}}
\else
\PackageError{pynum}{No output from Python. Check the logs (with --copy-logs)}{}
\fi
}

\newcommand{\limiti}[1]{%
\pynum[limiti]{#1}%
}
\newcommand{\limitist}[2][\st]{
\pynum[limiti]{subtask[#1]["#2"]}%
}

\newcommand{\stscore}[1]{%
\immediate\write18{cd \currfiledir; grep '^\#ST:' GEN | head -n #1 | tail -n 1 | cut -f 2 -d : | cut -f 1 -d \# | xargs echo -n > ../stscore.tex}%
\ifnum0\pdffilesize{stscore.tex}>0
\phantom{00}\llap{\input{stscore.tex}\unskip}%
\else
\PackageError{GEN}{Subtask #1 not found}{}
\fi
}

\newcommand{\st}{\the@subtaskcounter}

\newcommand{\subtask}{%
\addtocounter{@subtaskcounter}{1}%
\textbf{\makebox[2cm][l]{\kw@Subtask~\the@subtaskcounter}~[\stscore{\the@subtaskcounter} \kw@points]}:
}


%*********************************************************************
%                             Symbols                                *
%*********************************************************************

\gdef\Cpp{{C\nolinebreak[4]\hspace{-.05em}\raisebox{.4ex}{\tiny\bf ++}}}

%*********************************************************************
%                             Environments                           *
%*********************************************************************

% Function signature table
\newenvironment{signatures}
{\par\begin{mdframed}[backgroundcolor=gray!10]%
\begin{tabular}{@{}>{\centering\arraybackslash}p{2cm}|p{\dimexpr\textwidth-2cm-2\tabcolsep\relax}@{}}}
{\end{tabular}\end{mdframed}\par}

% contest
%
% Arguments: 1) Contest name
%            2) Contest location
%            3) Contest date

\newenvironment{contest}[3]{
    \gdef\this@contestname{#1}
    \gdef\this@contestlocation{#2}
    \gdef\this@contestdate{#3}
}{}

% problem
%
% Arguments: 1) Long title          e.g. Maximum subarray
%            2) Short title         e.g. maxsub
%            3) Input filename      e.g. stdin
%            4) Output filename     e.g. stdout
%            5) Time limit          e.g. 1 second
%            6) Memory limit        e.g. 256 Mib
%            7) Difficulty          e.g. 2
%            8) Syllabus level      (from 1 to 5)
%

\newenvironment{problem}[8]{
    \setcounter{figure}{0}

    % Store values so that they dont go lost
    \gdef\last@problemauthor{\this@problemauthor}
    \gdef\last@problemtitle{\this@problemtitle}
    \gdef\this@problemtitle{#1}
    \gdef\last@problemshorttitle{\this@problemshorttitle}
    \gdef\this@problemshorttitle{#2}
    \gdef\this@inputfilename{#3}
    \gdef\this@outputfilename{#4}
    \gdef\this@problemrevnumber{}
    \gdef\this@syllabuslevel{#8}

    % Header
    \if@showlogo\else\setlength{\logolength}{0mm}\setlength{\logomargin}{0mm}\fi
    \begin{tabular}{@{}p{\logolength+\logomargin}@{}p{\textwidth-\logolength-\logomargin}@{}}%
        \begin{minipage}[b]{\logolength}%
            \if@showlogo\includegraphics[width=\logolength]{\this@contestlogo}\fi
        \end{minipage} &%
           \begin{minipage}[b]{\textwidth-\logolength-\logomargin}%
            {\sffamily\Large
                \this@contestname%
                \hfill%
                \ifthenelse{\equal{\this@contestday}{}}%
                {}{\makebox[0pt][r]{\kw@Day~\this@contestday}}%
                \ifthenelse{\equal{\this@syllabuslevel}{}}%
                {}{\makebox[0pt][r]{\Level{\this@syllabuslevel}}}%
            }
            \vskip 2.5mm \hrule\vskip 2.5mm%
            {\sffamily\makebox[0pt][l]{%
                \this@contestlocation\unskip\ifthenelse{\equal{\this@contestdate}{}}{}{\ifthenelse{\equal{\this@contestlocation}{}}{}{, }\this@contestdate}}%
            \hfill\ttfamily\bfseries\makebox[0pt][r]{\this@problemshorttitle\enskip$\bullet$\enskip\@languagecode\ifthenelse{\equal{\this@problemrevnumber}{}}{}{\enskip$\bullet$\enskip\kw@revision \this@problemrevnumber%
            }}}
        \end{minipage}%
    \end{tabular}%

    \vskip 5mm

    % Problem title
    \noindent
    \refstepcounter{@problemcounter}
    \textbf{\problemheadfont\textsf{%
        \if@showproblemletter\kw@Problem~\Alph{@problemcounter}.\ \fi%
        \if@showproblemnumber\kw@Problem~\arabic{@problemcounter}.\ \fi%
        #1 (\texttt{#2})%
    }}%
    % Add a line to the toc now, so that it is in the right page
    \addcontentsline{toc}{section}{#1 (\texttt{#2})}

    \nopagebreak

    \problemtextfont
    % Make the summary table
    \if@showsummary
        \par\vspace{\afterproblemhead}
        \parindent=6.5mm
        \vbox{
            \begin{tabular}{l@{\extracolsep{1cm}}l}
                %\kw@InputFileName & \texttt{#3} \\
                %\kw@OutputFileName & \texttt{#4} \\
                \ifnum\pdfstrcmp{#5}{}=0 \else \kw@TimeLimit & #5~\kw@seconds\\ \fi
                \ifnum\pdfstrcmp{#6}{}=0 \else \kw@MemoryLimit & #6~MiB\\ \fi
                \ifnum\pdfstrcmp{#7}{}=0 \else \kw@Difficulty & #7\\ \fi
            \end{tabular}
        }
        \nopagebreak
    \fi
    \par\vspace{\afterconstraints}

    % Prepare for the problem statement
    \problemtextfont\parindent=0mm%
}{
    \@problemend
}

% solution
%
% it is shown only if showsolution is true

\NewEnviron{solution}{
\if@showsolutions
    \definecolor{backcolor}{gray}{0.95}
    \pagecolor{backcolor}

    \Solution
    \BODY

    \afterpage{\nopagecolor}
\fi
}

% example
%
% this is used for writing the sample testcases

\newenvironment{example}{
    \newcommand{\exmp}[2]{%
        \begin{minipage}[t]{\exmpwidinf}%
            \rightskip=0pt plus 1fill\relax%
            \ttfamily\obeylines\obeyspaces\frenchspacing%
            ##1%
        \end{minipage}&%
        \begin{minipage}[t]{\exmpwidouf}%
            \rightskip=0pt plus 1fill\relax%
            \ttfamily\obeylines\obeyspaces\frenchspacing%
            ##2%
        \end{minipage}\\%
        \hline%
    }%
    \newcommand{\exmpfile}[3][]{%
        \begin{minipage}[t]{\exmpwidinf}%
            \ifthenelse{\equal{##1}{nowrap}}{%
                \lstinputlisting[aboveskip=0pt,basicstyle=\ttfamily]{##2}%
            }{%
                \lstinputlisting[aboveskip=0pt,basicstyle=\ttfamily,breaklines]{##2}%
            }%
        \end{minipage}&%
        \begin{minipage}[t]{\exmpwidouf}%
            \ifthenelse{\equal{##1}{nowrap}}{%
                \lstinputlisting[aboveskip=0pt,basicstyle=\ttfamily]{##3}%
            }{%
                \lstinputlisting[aboveskip=0pt,basicstyle=\ttfamily,breaklines]{##3}%
            }%
        \end{minipage}\\%
        \hline%
    }%
    \begin{tabular}{|l|l|}
        \hline
        \multicolumn{1}{|c|}{\bf\texttt{\this@inputfilename}}&
        \multicolumn{1}{|c|}{\bf\texttt{\this@outputfilename}}\\
        \hline\hline
        \ignorespaces%
}{%
    \end{tabular}
}


% warning
%
% environment used to point out some warnings, e.g. about possible overflow
% issues (and suggestions on how to use modular arithmetic)

\newenvironment{warning}
  {\par\begin{mdframed}[linewidth=2pt,linecolor=gray]%
    \begin{list}{}{\leftmargin=1cm
                   \labelwidth=\leftmargin}\item[\Large\ding{43}]}
  {\end{list}\end{mdframed}\par}


% danger
%
% like warning, but more prominent (and red)

\newenvironment{danger}
{\par\begin{mdframed}[linewidth=2pt,linecolor=red!60!yellow,backgroundcolor=red!20!white]%
		\begin{list}{}{\leftmargin=1cm
				\labelwidth=\leftmargin}\item[\Large\ding{45}]}
		{\end{list}\end{mdframed}\par}


% end of cms-style.sty
\makeatother
\sloppy
