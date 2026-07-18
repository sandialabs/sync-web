LATEXMK ?= latexmk

.PHONY: all clean

all: whitepaper.pdf

whitepaper.pdf: main.tex references.bib sections/*.tex figures/*.tex tables/*.tex algorithms/*.tex sand_report_template/SANDreport.cls sand_report_template/*.pdf
	$(LATEXMK) -pdf -interaction=nonstopmode -halt-on-error -jobname=whitepaper main.tex

clean:
	$(LATEXMK) -C -jobname=whitepaper main.tex
