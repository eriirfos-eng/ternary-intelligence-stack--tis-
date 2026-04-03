# Building the Ternlang Whitepaper

## Requirements

```bash
# Ubuntu/Debian
sudo apt install texlive-full

# Or minimal install
sudo apt install texlive-latex-recommended texlive-science texlive-fonts-recommended
```

## Build to PDF

```bash
cd whitepaper/
pdflatex ternlang-whitepaper.tex
bibtex ternlang-whitepaper
pdflatex ternlang-whitepaper.tex
pdflatex ternlang-whitepaper.tex
```

The final PDF is `ternlang-whitepaper.pdf`.

## Submit to arXiv

1. Create account at https://arxiv.org
2. Submit to: **cs.PL** (Programming Languages) with cross-list to:
   - cs.AR (Hardware Architecture)
   - cs.NE (Neural and Evolutionary Computing)
3. Upload: `ternlang-whitepaper.tex` + `references.bib`

4. arXiv will compile the PDF on their servers

## Target venues (beyond arXiv)

- **PLDI** — Programming Language Design and Implementation
- **ISCA** — International Symposium on Computer Architecture
- **DATE** — Design, Automation & Test in Europe (for HDL section)
- **NeurIPS Workshop** — Machine Learning and Compression (for sparse inference)
