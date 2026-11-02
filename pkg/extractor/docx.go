package extractor

import (
	"archive/zip"
	"bytes"
	"encoding/xml"
	"errors"
	"io"
	"strings"
)

// ExtractDocx extracts text from a Microsoft Word .docx file using pure Go standard library.
func ExtractDocx(filePath string) (*Result, error) {
	reader, err := zip.OpenReader(filePath)
	if err != nil {
		return nil, err
	}
	defer reader.Close()

	var documentXML *zip.File
	for _, file := range reader.File {
		if file.Name == "word/document.xml" {
			documentXML = file
			break
		}
	}

	if documentXML == nil {
		return nil, errors.New("invalid docx: word/document.xml not found")
	}

	rc, err := documentXML.Open()
	if err != nil {
		return nil, err
	}
	defer rc.Close()

	text, err := parseDocxXML(rc)
	if err != nil {
		return nil, err
	}

	return &Result{
		Content: text,
		IsOCR:   false,
	}, nil
}

// parseDocxXML parses document.xml extracting all text nodes within paragraphs.
func parseDocxXML(r io.Reader) (string, error) {
	decoder := xml.NewDecoder(r)
	var sb strings.Builder
	var inTextNode bool

	for {
		token, err := decoder.Token()
		if err == io.EOF {
			break
		}
		if err != nil {
			return "", err
		}

		switch elem := token.(type) {
		case xml.StartElement:
			if elem.Name.Local == "t" {
				inTextNode = true
			} else if elem.Name.Local == "p" || elem.Name.Local == "tr" {
				sb.WriteString("\n")
			} else if elem.Name.Local == "tab" {
				sb.WriteString("\t")
			}
		case xml.EndElement:
			if elem.Name.Local == "t" {
				inTextNode = false
			}
		case xml.CharData:
			if inTextNode {
				sb.Write(bytes.TrimSpace(elem))
				sb.WriteString(" ")
			}
		}
	}

	return strings.TrimSpace(sb.String()), nil
}
