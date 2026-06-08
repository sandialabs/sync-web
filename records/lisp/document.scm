(define-class (document)
  ;; Document class stores a durable byte-vector value with symbol-keyed metadata.

  (define-method (*type* self)
    ;; Return the public object type.
    ;;   Returns:
    ;;     symbol: `document`.
    'document)

  (define-method (*init* self value (meta '()))
    ;; Initialize document value and optional metadata.
    ;;   Args:
    ;;     value (byte-vector): initial document payload.
    ;;     meta (alist): optional symbol-keyed metadata dictionary.
    ;;   Returns:
    ;;     boolean: #t after setting fields.
    (set! (self '(1)) (sync-cons ((self '~meta-encode) meta)
                                 ((self '~value-encode) value))))

  (define-method (get self key)
    ;; Get document content or metadata.
    ;;   Args:
    ;;     key (symbol): supported keys are `value` and `meta`.
    ;;   Returns:
    ;;     any: decoded value, metadata alist, or `(unknown)` for unavailable data.
    (case key
      ((value) ((self '~value-decode) (self '(1 1))))
      ((meta) ((self '~meta-decode) (self '(1 0))))
      (else (error 'key-error "Document does not contain key: ~S" key))))

  (define-method (set! self key value)
    ;; Update document content or metadata.
    ;;   Args:
    ;;     key (symbol): supported keys are `value` and `meta`.
    ;;     value: byte-vector replacement content, metadata patch, `()`, or `(nothing)`.
    ;;   Returns:
    ;;     boolean: #t after mutation or no-op metadata update.
    (case key
      ((value) (set! (self '(1 1)) ((self '~value-encode) value)))
      ((meta) (cond ((null? value) #t)
                    ((equal? value '(nothing)) (set! (self '(1 0)) ((self '~meta-encode) '())))
                    (else (set! (self '(1 0))
                                ((self '~meta-encode)
                                 ((self '~meta-patch) ((self 'get) 'meta) value))))))
      (else (error 'key-error "Document does not contain key: ~S" key))))

  (define-method (slice! self key)
    ;; Slice document state to reveal one top-level field.
    ;;   Args:
    ;;     key (symbol): field to retain, `value` or `meta`.
    ;;   Returns:
    ;;     boolean: #t after mutation.
    (case key
      ((value) (if (not (sync-null? (self '(1 0))))
                   (set! (self '(1 0)) (sync-cut (self '(1 0))))
                   #t))
      ((meta) (set! (self '(1 1)) (sync-cut (self '(1 1)))))
      (else (error 'key-error "Document does not contain key: ~S" key))))

  (define-method (prune! self key)
    ;; Prune document state to hide one top-level field.
    ;;   Args:
    ;;     key (symbol): field to hide, `value` or `meta`.
    ;;   Returns:
    ;;     boolean: #t after mutation.
    (case key
      ((value) (set! (self '(1 1)) (sync-cut (self '(1 1)))))
      ((meta) (if (not (sync-null? (self '(1 0))))
                  (set! (self '(1 0)) (sync-cut (self '(1 0))))
                  #t))
      (else (error 'key-error "Document does not contain key: ~S" key))))

  (define-method (~value-encode self value)
    ;; Validate and store a document payload.
    ;;   Args:
    ;;     value (byte-vector): raw document payload.
    ;;   Returns:
    ;;     byte-vector: payload bytes.
    (if (byte-vector? value) value
        (error 'value-error "Document value must be a byte-vector: ~S" value)))

  (define-method (~value-decode self node)
    ;; Decode a document payload.
    ;;   Args:
    ;;     node (sync node): payload node.
    ;;   Returns:
    ;;     byte-vector: document payload, or `(unknown)` for stubs.
    (cond ((sync-stub? node) '(unknown))
          ((byte-vector? node) node)
          (else (error 'encoding-error "Document value is not encoded as bytes: ~S" node))))

  (define-method (~meta-encode self meta)
    ;; Encode a metadata dictionary.
    ;;   Args:
    ;;     meta (alist): symbol-keyed metadata dictionary.
    ;;   Returns:
    ;;     byte-vector: encoded metadata expression.
    (begin ((self '~meta-validate) meta)
           (expression->byte-vector meta)))

  (define-method (~meta-decode self node)
    ;; Decode a metadata dictionary.
    ;;   Args:
    ;;     node (sync node): encoded metadata node.
    ;;   Returns:
    ;;     list: metadata alist, `()`, or `(unknown)` for stubs.
    (cond ((sync-stub? node) '(unknown))
          ((sync-null? node) '())
          ((not (byte-vector? node)) (error 'encoding-error "Document metadata is not encoded as bytes: ~S" node))
          (else (let ((meta (byte-vector->expression node)))
                  ((self '~meta-validate) meta)
                  meta))))

  (define-method (~meta-patch self meta patch)
    ;; Apply a metadata patch to an existing dictionary.
    ;;   Args:
    ;;     meta (alist): current metadata dictionary.
    ;;     patch (alist): symbol-keyed metadata patch.
    ;;   Returns:
    ;;     list: patched metadata dictionary.
    (begin ((self '~meta-validate) meta)
           ((self '~meta-validate) patch)
           (let loop ((patch patch) (meta meta))
             (if (null? patch) meta
                 (let* ((entry (car patch))
                        (key (car entry))
                        (value (cadr entry))
                        (rest (let remove ((meta meta))
                                (cond ((null? meta) '())
                                      ((eq? key (caar meta)) (remove (cdr meta)))
                                      (else (cons (car meta) (remove (cdr meta))))))))
                   (loop (cdr patch)
                         (if (equal? value '(nothing)) rest
                             (cons `(,key ,value) rest))))))))

  (define-method (~meta-validate self meta)
    ;; Validate metadata dictionary shape.
    ;;   Args:
    ;;     meta (alist): metadata dictionary or patch.
    ;;   Returns:
    ;;     boolean: #t when valid.
    (let loop ((meta meta))
      (cond ((not (list? meta)) (error 'metadata-error "Metadata must be a list: ~S" meta))
            ((null? meta) #t)
            ((not (and (list? (car meta))
                       (= (length (car meta)) 2)
                       (symbol? (caar meta))))
             (error 'metadata-error "Metadata entries must be symbol/value pairs: ~S" (car meta)))
            (else (loop (cdr meta)))))))
