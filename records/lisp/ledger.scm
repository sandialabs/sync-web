(define-class (ledger)
  ;; Ledger class manages config, staged state, and signed/pinned chain history.
  (define-method (*init* self standard (config '()) tree-class chain-class)
    ;; Initialize ledger with helper objects, inline config data, and fresh storage classes.
    ((self '~field!) 'standard standard)
    ((self '~field!) 'config (expression->byte-vector config))
    (let ((standard-obj (sync-eval standard)))
      ((self '~field!) 'stage ((standard-obj 'init) tree-class))
      ((self '~field!) 'temp ((standard-obj 'init) chain-class))
      ((self '~field!) 'perm ((standard-obj 'init) chain-class))))

  (define-method (~child self node operation . arguments)
    (((sync-eval ((self '~field!) 'standard)) 'deep-call) node '()
     `(lambda (object)
        ((object ',operation)
         ,@(map (lambda (argument) `',argument) arguments)))))

  (define-method (~chain-indices self node)
    ;; Negotiate a contained structural inventory with an authenticated Chain.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (result
            ((standard 'deep-call) node '()
             '(lambda (chain)
                (if (member 'indices (chain '*api*)) ((chain 'indices))
                    (let ((size ((chain 'size))))
                      (if (not (and (integer? size) (>= size 0)))
                          (error 'integrity-error "Invalid historical Chain size"))
                      (let loop ((index 0) (indexes '()) (complete? #t))
                        (if (= index size) `(chain ,(reverse indexes) ,complete?)
                            (let ((available?
                                   (not (equal? ((chain 'get) index) '(unknown)))))
                              (loop (+ index 1)
                                    (if available? (cons index indexes) indexes)
                                    (and complete? available?)))))))))))
      (if (not (and (list? result) (= (length result) 3)
                    (eq? (car result) 'chain) (list? (cadr result))
                    (boolean? (caddr result))
                    (let loop ((indexes (cadr result)) (previous -1))
                      (or (null? indexes)
                          (and (integer? (car indexes)) (> (car indexes) previous)
                               (loop (cdr indexes) (car indexes)))))))
          (error 'integrity-error "Invalid Chain inventory: ~S" result))
      result))

  (define-method (~chain-latest-materialized-index self node)
    ;; Resolve the latest available payload through the contained Chain's own behavior.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (index
            ((standard 'deep-call) node '()
             '(lambda (chain)
                (let ((size ((chain 'size))))
                  (if (not (and (integer? size) (>= size 0)))
                      (error 'integrity-error "Invalid historical Chain size"))
                  (let loop ((index (- size 1)))
                    (cond ((< index 0)
                           (error 'availability-error
                                  "Retained Chain has no permanent payload"))
                          ((equal? ((chain 'get) index) '(unknown))
                           (loop (- index 1)))
                          (else index))))))))
      (if (not (and (integer? index) (>= index 0)))
          (error 'integrity-error "Invalid latest permanent Chain index: ~S" index))
      index))

  (define-method (config self (path '()))
    (cond ((and (pair? path) (eq? (car path) 'peer-preapproval))
           ((self '~config-get) (append '(private bridge-preapproval) (cdr path))))
          ((equal? path '(journal-rotation-indexes))
           ((self '~config-get) '(private journal rotation-indexes)))
          ((and (pair? path) (eq? (car path) 'private))
           (error 'config-error "Private Ledger config is not public"))
          (else ((self '~config-get) path))))

  (define-method (descriptor self index)
    ;; Return the public descriptor represented by the selected local state.
    ;; The current implementation publishes descriptor values through committed
    ;; Ledger state; callers use `index` to select the corresponding head.
    (if (not (integer? index))
        (error 'index-error "Descriptor index must be an integer: ~S" index))
    ((self '~config-get) '(public)))

  (define-method (size self)
    ((self '~child) ((self '~field!) 'perm) 'size))

  (define-method (truncate! self index)
    ;; Irreversibly release locally available history through an inclusive index.
    ;; Permanent and temporary chains are replaced only after both candidates
    ;; truncate successfully.
    ;;   Args:
    ;;     index (integer): inclusive positive or negative history cutoff.
    ;;   Returns:
    ;;     boolean: #t after mutation.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (normalized ((self '~child) ((self '~field!) 'perm) 'index index))
           (perm ((self '~field!) 'perm))
           (temp ((self '~field!) 'temp)))
      (if (not (= normalized
                  ((self '~child) temp 'index index)))
          (error 'integrity-error "Ledger history chains are misaligned"))
      (set! perm
            (cadr
             ((standard 'deep-call!) perm '()
              `(lambda (chain) ((chain 'truncate!) ,normalized)))))
      (set! temp
            (cadr
             ((standard 'deep-call!) temp '()
              `(lambda (chain) ((chain 'truncate!) ,normalized)))))
      ((self '~field!) 'perm perm)
      ((self '~field!) 'temp temp)
      #t))

  (define-method (~resolve-permanent-path self source path)
    ;; Resolve each latest selection against its containing permanent Chain.
    (if (not (and (list? path) (pair? path) (integer? (car path))))
        (error 'integrity-error "Invalid retained history path: ~S" path))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (requested (car path))
           (index (if (= requested -1)
                      ((self '~chain-latest-materialized-index) source)
                      requested))
           (remaining (cdr path)))
      (if (null? remaining) (list index)
          (let ((tree-path (car remaining)))
            (if (not (list? tree-path))
                (error 'integrity-error "Invalid retained history path: ~S" path))
            (if (null? (cdr remaining)) (list index tree-path)
                (let* ((head ((standard 'deep-get) source (list index)))
                       (nested (and (sync-node? head)
                                    ((standard 'deep-get) head (list tree-path)))))
                  (if (or (not (sync-node? nested))
                          (equal? nested '(nothing))
                          (equal? nested '(unknown)))
                      (error 'availability-error
                             "Retained Chain is unavailable: ~S" tree-path))
                  (append (list index tree-path)
                          ((self '~resolve-permanent-path)
                           nested (cdr remaining)))))))))

  (define-method (read self index supplied-object (required-paths #f))
    ;; Anchor sparse supplied evidence at an exact permanent-history index and,
    ;; when requested by authenticated orchestration, enrich only exact internal
    ;; paths. Never construct the complete permanent/temporary union.
    (if (or (not (and (sync-node? supplied-object)
                      (sync-pair? supplied-object)))
            (< index 0)
            (>= index ((self 'size)))
            (and required-paths (not (list? required-paths))))
        (error 'integrity-error "Invalid local history object"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (local ((self '~child) ((self '~field!) 'perm) 'previous index))
           (resolve-latest?
            (and (list? required-paths) (= (length required-paths) 2)
                 (pair? (car required-paths))
                 (eq? (caar required-paths) 'paths)
                 (pair? (cadr required-paths))
                 (eq? (caadr required-paths) 'resolve-latest-perm?)
                 (eq? (cadr (cadr required-paths)) #t)))
           (required-paths
            (if resolve-latest? (cadr (assoc 'paths required-paths)) required-paths))
           (window ((self '~config-get) '(public window)))
           (latest (- ((self 'size)) 1))
           (paths (cond ((or (not required-paths) (null? required-paths)) '())
                        ((not (list? (car required-paths)))
                         (list required-paths))
                        (else required-paths)))
           (resolved-paths
            (if resolve-latest?
                (map (lambda (path)
                       ((self '~resolve-permanent-path) local path))
                     paths)
                paths))
           ;; Enrich each exact path once, but preserve the slot-aligned result.
           (evidence-paths
            (let loop ((paths resolved-paths) (out '()))
              (if (null? paths) (reverse out)
                  (loop (cdr paths)
                        (if (member (car paths) out)
                            out (cons (car paths) out)))))))
      (if (not (equal? (sync-digest supplied-object) (sync-digest local)))
          (error 'integrity-error "Object is not contained in local history"))
      (let* ((temp
              (and (not resolve-latest?) (pair? evidence-paths)
                   (or (not window) (> index (- latest window)))
                   ((self '~child) ((self '~field!) 'temp)
                    'previous index)))
             (result
              (let loop ((result supplied-object) (remaining evidence-paths))
                (if (null? remaining) result
                    (let* ((path (car remaining))
                           (temp-source?
                            (and temp
                                 (equal? (sync-digest local) (sync-digest temp))
                                 (not (equal? ((standard 'deep-get) temp path)
                                              '(unknown)))))
                           (source (if temp-source? temp local))
                           (source-known?
                            (not (equal? ((standard 'deep-get) source path)
                                         '(unknown)))))
                      (if (and (not source-known?)
                               (equal? ((standard 'deep-get) result path) '(unknown)))
                          (error 'integrity-error
                                 "Required history evidence is unavailable: ~S" path))
                      (loop
                       (if source-known?
                           ((standard 'deep-merge!) result
                            ((standard 'deep-slice!) source path))
                           result)
                       (cdr remaining)))))))
        (if resolve-latest?
            `((object ,result) (paths ,resolved-paths))
            result))))

  (define-method (~anchor self index head)
    ;; Anchor a supplied head at the exact index already carried by its
    ;; authenticated committed path; never rediscover it by scanning history.
    (if (not (integer? index))
        (error 'integrity-error "History anchor requires an exact index"))
    ((self 'read) index head))

  (define-method (~identity-verify self identity)
    ;; Validate and return a journal's stable SHA-256 identity commitment.
    (let ((id (and (list? identity) (assoc 'id identity)
                   (cadr (assoc 'id identity))))
          (nonce (and (list? identity) (assoc 'nonce identity)
                      (cadr (assoc 'nonce identity)))))
      (if (not (and (list? identity) (= (length identity) 2)
                    (byte-vector? id) (= (length id) 32)
                    (byte-vector? nonce) (= (length nonce) 32)
                    (equal? id
                            (sync-hash
                             (expression->byte-vector
                              (list 'sync-web/journal-id/v1 nonce))))))
          (error 'identity-error "Invalid journal identity commitment: ~S" identity))
      id))

  (define-method (~rotation-verify self version identity rotation previous-key)
    ;; Verify one versioned key transition from an already accepted public key.
    (let* ((identity-id (and (= version 1) ((self '~identity-verify) identity)))
           (field (lambda (key) (and (list? rotation) (assoc key rotation)
                                     (cadr (assoc key rotation)))))
           (index (field 'index))
           (previous-index (field 'previous-index))
           (included-previous (field 'previous-key))
           (public-key (field 'public-key))
           (signature (field 'signature)))
      (if (not (and (memv version '(1 2)) (list? rotation) (= (length rotation) 5)
                    (integer? index) (integer? previous-index) (< previous-index index)
                    (byte-vector? included-previous) (byte-vector? public-key)
                    (byte-vector? signature)))
          (error 'integrity-error "Malformed journal signing-key transition: ~S" rotation))
      (if (not (equal? included-previous previous-key))
          (error 'integrity-error
                 "Journal signing-key transition does not start at the accepted key: ~S" index))
      (if (not (crypto-verify
                previous-key signature
                (expression->byte-vector
                 (if (= version 1)
                     (list 'sync-web/journal-key-rotation/v1 identity-id index previous-index
                           previous-key public-key)
                     (list 'sync-web/journal-key-rotation/v2 index previous-index
                           previous-key public-key)))))
          (error 'integrity-error
                 "Journal signing-key transition signature does not verify: ~S" index))
      public-key))

  (define-method (~rotate-key! self previous-key public-key signature)
    ;; Stage one derivation-salt-bound signing-key transition for an atomic step.
    (let* ((configured-key ((self '~config-get) '(public journal public-key)))
           (indexes ((self '~config-get) '(private journal rotation-indexes)))
           (index ((self 'size)))
           (previous-index (if (null? indexes) -1 (car (reverse indexes))))
           (rotation `((index ,index) (previous-index ,previous-index)
                       (previous-key ,previous-key) (public-key ,public-key)
                       (signature ,signature))))
      (if (not (equal? configured-key previous-key))
          (error 'key-rotation-error "Root secret does not match the active journal signing key"))
      (if (not (null? ((self '~config-get) '(private journal pending-rotation))))
          (error 'key-rotation-error "A journal signing-key rotation is already pending commitment"))
      (if (equal? previous-key public-key) #t
          (begin
            ((self '~rotation-verify) 2 #f rotation previous-key)
            ((self '~config-set!) '(private journal pending-rotation) rotation)
            #t))))

  (define-method (~store-peer! self name verified)
    ;; Store one alias-scoped reciprocal peer checkpoint without durable identity binding.
    (let ((public-key (cadr (assoc 'public-key verified)))
          (existing ((self '~config-get) `(private bridge ,name))))
      (if (not (byte-vector? public-key))
          (error 'bridge-key-error "Bridge descriptor lacks a journal signing key: ~S" name))
      (if (null? existing)
          (begin
            ((self '~config-set!) `(private bridge ,name public-key) public-key)
            ((self '~config-set!) `(private bridge-retired ,name) '())))
      #t))

  (define-method (~get self path)
    (((sync-eval ((self '~field!) 'standard)) 'deep-get)
     ((self '~field!) 'stage) ((self '~path-normalize) path #t #f)))

  (define-method (~exercise self method arguments)
    ;; Build the exact opaque object exercise used by staged and historical paths.
    (if (not (list? arguments))
        (error 'argument-error "Resource arguments must be a proper list"))
    (if (and method (not (null? method)) (not (symbol? method)))
        (error 'argument-error "Resource method must be a symbol"))
    (if (and (or (not method) (null? method)) (pair? arguments))
        (error 'argument-error "Blank resource method requires blank arguments"))
    `(lambda (running)
       ,(if (or (not method) (null? method))
            '`((class ,(running '*name*))
               (object-hash ,(sync-digest (running)))
               (code-hash ,(sync-digest (sync-car (running)))))
            (if (eq? method '*api*)
                `(if (procedure? running)
                     (running '*api*)
                     (error 'object-error "Resource is not an object"))
                `(apply (running ',method) ',arguments)))))

  (define-method (~object-code self standard stage path)
    ;; Return a live resource's code digest through the existing shared boundary.
    (car
     ((standard 'deep-call!) stage path
      '(lambda (running)
         (sync-digest (sync-car (running)))))))

  (define-method (put! self path value (object? #f) (expected? #f) (expected #f))
    ;; Stage inert content or one uninitialized Standard object shell.
    (if (and (not object?)
             (not (or (byte-vector? value) (equal? value '(nothing)))))
        (error 'value-error "Expected bytes or (nothing)"))
    (if (and (not object?) expected?
             (not (or (byte-vector? expected) (equal? expected '(nothing)))))
        (error 'value-error "Expected comparison bytes or (nothing)"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (normalized-path ((self '~path-normalize) path #t #t))
           (current ((self '~get) path))
           (matches?
            (or (not expected?)
                (if object?
                    (if (equal? expected '(nothing))
                        (equal? current '(nothing))
                        (and (sync-node? current)
                             (equal?
                              ((self '~object-code) standard
                               ((self '~field!) 'stage) normalized-path)
                              (sync-digest (sync-car ((standard 'make) expected))))))
                    (equal? current expected)))))
      (if (not matches?) #f
          (let* ((stage ((self '~field!) 'stage))
                 (stage
                  (if object?
                      ((standard 'deep-set!) stage normalized-path
                       ((standard 'make) value))
                      (if (> (length normalized-path) 2)
                          ((standard 'deep-set!) stage normalized-path value)
                          (cadr
                           ((standard 'deep-call!) stage '()
                            `(lambda (tree)
                               ((tree 'set!) ',(car normalized-path) ',value))))))))
            ((self '~field!) 'stage
             (cadr
              ((standard 'deep-call!) stage '()
               `(lambda (tree)
                  ((tree 'copy!) '(*transition*) '(*transition* previous))
                  ((tree 'set!) '(*transition* operation)
                   '((function put!) (path ,path) (value ,value)))))))))))

  (define-method (use! self path (method #f) (arguments '()) (read-only? #f))
    ;; Exercise staged content and optionally discard an object successor.
    (if (not (boolean? read-only?))
        (error 'argument-error "read-only? must be boolean"))
    (let ((value ((self '~get) path)))
      (if (not (or (sync-node? value)
                   (and (pair? value) (eq? (car value) 'sync-node))))
          (begin
            (if (or method (pair? arguments))
                (error 'argument-error "Inert use requires blank method and arguments"))
            value)
          (let* ((standard-node ((self '~field!) 'standard))
                 (stage ((self '~field!) 'stage))
                 (path-normalized ((self '~path-normalize) path #t #t))
                 (exercise ((self '~exercise) method arguments))
                 (called
                  (((sync-eval standard-node) 'deep-call!)
                   stage path-normalized exercise))
                 (successor (cadr called)))
            (if (and (not read-only?)
                     (not (equal? (sync-digest stage) (sync-digest successor))))
                ((self '~field!) 'stage
                 (cadr
                  (((sync-eval standard-node) 'deep-call!) successor '()
                   `(lambda (tree)
                      ((tree 'copy!) '(*transition*) '(*transition* previous))
                      ((tree 'set!) '(*transition* operation)
                       '((function use!) (path ,path) (method ,method))))))))
            (car called)))))

  (define-method (use-batch! self requests (read-only? #f))
    ;; Exercise ordered requests and optionally discard every successor.
    (if (not (boolean? read-only?))
        (error 'argument-error "read-only? must be boolean"))
    (if (not (and (list? requests) (<= (length requests) 1024)
                  (let loop ((requests requests))
                    (or (null? requests)
                        (and (list? (car requests))
                             (= (length (car requests)) 3)
                             (list? (caddar requests))
                             (loop (cdr requests)))))))
        (error 'argument-error "Malformed resource batch"))
    (map (lambda (request)
           ((self 'use!) (car request) (cadr request) (caddr request)
            read-only?))
         requests))

  (define-method (copy! self source path (expected? #f) (expected #f))
    ;; Copy raw staged content atomically from one pre-mutation snapshot.
    (if (and expected?
             (not (or (byte-vector? expected)
                      (equal? expected '(nothing)))))
        (error 'value-error "Expected comparison bytes or (nothing)"))
    ((self '~path-normalize) source #t #t)
    ((self '~path-normalize) path #t #t)
    (let ((value ((self '~get) source)))
      (if (or (equal? value '(unknown))
              (and (pair? value) (eq? (car value) 'directory)
                   (not (caddr value))))
          (error 'availability-error "Copy source is unavailable: ~S" source))
      (if (and expected? (not (equal? ((self '~get) path) expected))) #f
          (let ((standard (sync-eval ((self '~field!) 'standard))))
            ((self '~field!) 'stage
             (cadr
              ((standard 'deep-call!) ((self '~field!) 'stage) '()
               `(lambda (tree)
                  ((tree 'copy-batch!) '(,source) '(,path))
                  ((tree 'copy!) '(*transition*) '(*transition* previous))
                  ((tree 'set!) '(*transition* operation)
                   '((source ,source) (path ,path)))))))))))

  (define-method (copy-batch! self sources paths (expected? #f) (expected '()))
    ;; Copy ordered raw sources and compare targets from one staged snapshot.
    (if (not (and (list? sources) (list? paths)
                  (= (length sources) (length paths))
                  (or (not expected?)
                      (and (list? expected)
                           (= (length paths) (length expected))))))
        (error 'argument-error
               "Copy sources, paths, and expected values must have equal lengths"))
    (if (> (length sources) 1024)
        (error 'argument-error "Copy batch count exceeds 1024"))
    (if (and expected?
             (not (let loop ((values expected))
                    (or (null? values)
                        (and (or (byte-vector? (car values))
                                 (equal? (car values) '(nothing)))
                             (loop (cdr values)))))))
        (error 'value-error "Expected comparison bytes or (nothing)"))
    (for-each (lambda (source) ((self '~path-normalize) source #t #t)) sources)
    (for-each (lambda (path) ((self '~path-normalize) path #t #t)) paths)
    (let ((values (map (lambda (source) ((self '~get) source)) sources)))
      (for-each
       (lambda (source value)
         (if (or (equal? value '(unknown))
                 (and (pair? value) (eq? (car value) 'directory)
                      (not (caddr value))))
             (error 'availability-error "Copy source is unavailable: ~S" source)))
       sources values)
      (if (and expected?
               (let loop ((paths paths) (expected expected))
                 (and (pair? paths)
                      (or (not (equal? ((self '~get) (car paths)) (car expected)))
                          (loop (cdr paths) (cdr expected))))))
          #f
          (if (null? sources) #t
              (let ((standard (sync-eval ((self '~field!) 'standard))))
                ((self '~field!) 'stage
                 (cadr
                  ((standard 'deep-call!) ((self '~field!) 'stage) '()
                   `(lambda (tree)
                      ((tree 'copy-batch!) ',sources ',paths)
                      ((tree 'copy!) '(*transition*) '(*transition* previous))
                      ((tree 'set!) '(*transition* operation)
                       '((sources ,sources) (paths ,paths))))))))))))

  (define-method (put-batch! self changes)
    ;; Compare one snapshot, then stage ordered inert/object replacements.
    (if (not (and (list? changes) (<= (length changes) 1024)
                  (let loop ((changes changes))
                    (or (null? changes)
                        (and (list? (car changes))
                             (or (= (length (car changes)) 3)
                                 (= (length (car changes)) 4))
                             (boolean? (caddr (car changes)))
                             (loop (cdr changes)))))))
        (error 'argument-error "Malformed put batch"))
    (for-each
     (lambda (change) ((self '~path-normalize) (car change) #t #t))
     changes)
    (let ((standard (sync-eval ((self '~field!) 'standard))))
      (if (let loop ((changes changes))
            (and (pair? changes)
                 (or
                  (and (= (length (car changes)) 4)
                       (let* ((change (car changes))
                              (object? (caddr change))
                              (current ((self '~get) (car change)))
                              (expected (cadddr change)))
                         (not
                          (if object?
                              (and (sync-node? current)
                                   (equal?
                                    ((self '~object-code) standard
                                     ((self '~field!) 'stage)
                                     ((self '~path-normalize)
                                      (car change) #t #t))
                                    (sync-digest
                                     (sync-car ((standard 'make) expected)))))
                              (equal? current expected)))))
                  (loop (cdr changes)))))
          #f
          (begin
            (for-each
             (lambda (change)
               ((self 'put!) (car change) (cadr change) (caddr change)))
             changes)
            #t))))

  (define-method (~retrieve self path pinned? proof? head ancestor? (method #f) (arguments '()))
    (let* ((trace-path path)
           (path ((self '~path-normalize) path #f #f))
           (standard (sync-eval ((self '~field!) 'standard)))
           (head (if head head ((self '~head) path)))
           (content ((standard 'deep-get) head path))
           (resource? (sync-node? content))
           (last (and (pair? path) (car (reverse path))))
           (chain-boundary?
            (and (pair? last) (= (length last) 3)
                 (eq? (car last) '*bridge*) (eq? (caddr last) 'chain)))
           (reserved?
            (lambda (name)
              (and (symbol? name)
                   (let ((text (symbol->string name)))
                     (and (> (length text) 1) (char=? (text 0) #\*)
                          (char=? (text (- (length text) 1)) #\*))))))
           (project
            (lambda (value)
              (let loop ((entries (cadr value)) (visible '()))
                (if (null? entries)
                    `(directory ,(reverse visible) ,(caddr value))
                    (loop (cdr entries)
                          (if (reserved? (caar entries)) visible
                              (cons (car entries) visible)))))))
           (directory
            (and ancestor? (pair? content) (eq? (car content) 'directory)
                 (pair? (cdr content)) (list? (cadr content)) content))
           (content
            (cond (directory (project directory))
                  (ancestor? (error 'authorization-error
                                    "Ancestor access requires a directory"))
                  ((and resource? chain-boundary?
                        (or (not method) (null? method)) (null? arguments))
                   ((self '~chain-indices) content))
                  (resource?
                   (car
                    ((standard 'deep-call!) head path
                     ((self '~exercise) method arguments))))
                  ((or method (pair? arguments))
                   (error 'argument-error "Inert retrieve requires blank method and arguments"))
                  (else content)))
           (proof
            (and proof?
                 (cond
                  ((and resource? (not directory))
                   ((standard 'deserialize)
                    ((self '~trace) 0 trace-path head method arguments #t)))
                  ((not directory) ((standard 'deep-slice!) head path))
                  (else
                     (let* ((parts (reverse path))
                            (parent (reverse (cdr parts)))
                            (prefix (car parts))
                            (node head))
                       (for-each
                        (lambda (entry)
                          (set! node
                                (cadr
                                 ((standard 'deep-call!) node parent
                                  `(lambda (tree)
                                     ((tree 'prune!)
                                      ',(append prefix (list (car entry)))
                                      ,(not (reserved? (car entry)))))))))
                        (cadr directory))
                       ((standard 'deep-slice!) node path))))))
           (content
            (if (and directory proof?)
                (project ((standard 'deep-get) proof path)) content)))
      (if (or (not (or pinned? proof?))
              (and pinned? (not proof?) (equal? content '(unknown))))
          content
          `((content ,content)
            ,@(if pinned?
                  `((pinned?
                     ,(not (equal?
                            ((standard 'deep-get)
                             ((self '~field!) 'perm) path)
                            '(unknown))))) '())
            ,@(if proof? `((proof ,((standard 'serialize) proof))) '())
            ,@(if ancestor? '((ancestor? #t)) '())))))

  (define-method (retrieve-batch self paths (pinned? #f) (heads #f)
                                (ancestors #f) (proof? #f)
                                (methods #f) (arguments #f))
    ;; Retrieve ordered committed paths and optionally construct one union proof.
    (if (not (and (list? paths)
                  (or (not heads)
                      (and (list? heads) (= (length heads) (length paths))))
                  (or (not ancestors)
                      (and (list? ancestors)
                           (= (length ancestors) (length paths))))
                  (or (not methods)
                      (and (list? methods) (= (length methods) (length paths))))
                  (or (not arguments)
                      (and (list? arguments) (= (length arguments) (length paths))
                           (not (member #f (map list? arguments)))))))
        (error 'argument-error "Invalid retrieve batch shape"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (heads (if heads heads (map (lambda (path) #f) paths)))
           (ancestors
            (if ancestors ancestors (map (lambda (path) #f) paths)))
           (methods (if methods methods (map (lambda (path) #f) paths)))
           (arguments (if arguments arguments (map (lambda (path) '()) paths)))
           ;; Ancestor projections require scalar pruning. Exact paths use the
           ;; serializer's native union-of-accesses traversal instead.
           (slice-proof? proof?))
      (let loop ((remaining-paths paths)
                 (remaining-heads heads)
                 (remaining-ancestors ancestors)
                 (remaining-methods methods)
                 (remaining-arguments arguments)
                 (results '())
                 (proof #f))
        (if (null? remaining-paths)
            (if proof?
                (let* ((head (and (pair? heads) (car heads)))
                       (normalized
                        (and (pair? paths)
                             ((self '~path-normalize) (car paths) #f #f)))
                       (indexed-head?
                        (and (list? head) (assoc 'index head)
                             (assoc 'object head)))
                       (head
                        (cond
                         (indexed-head?
                          ((self '~anchor)
                           (cadr (assoc 'index head))
                           (cadr (assoc 'object head))))
                         (head head)
                         (else #f)))
                       (serialization
                        (cond
                         (proof ((standard 'serialize) proof))
                         ((null? paths) #f)
                         (else ((self 'trace-batch) paths head)))))
                  `((results ,(reverse results))
                    (proof ,serialization)))
                (reverse results))
            (let* ((retrieved
                    ((self 'retrieve) (car remaining-paths) #f slice-proof?
                     (car remaining-heads) (car remaining-ancestors)
                     (car remaining-methods) (car remaining-arguments)))
                   (content
                    (if slice-proof?
                        (cadr (assoc 'content retrieved)) retrieved))
                   (slice
                    (and slice-proof?
                         ((standard 'deserialize)
                          (cadr (assoc 'proof retrieved)))))
                   (proof
                    (cond ((not slice-proof?) #f)
                          ((not proof) slice)
                          (else ((standard 'deep-merge!) slice proof)))))
              (loop
               (cdr remaining-paths) (cdr remaining-heads)
               (cdr remaining-ancestors)
               (cdr remaining-methods) (cdr remaining-arguments)
               (cons
                `((content ,content)
                  ,@(if pinned?
                        `((pinned? ,((self 'pinned?)
                                     (car remaining-paths)))) '()))
                results)
               proof))))))

  (define-method (~peer-head self path (index -1) (remote-index -1))
    ;; Describe a committed bridge edge at a selected local/remote index.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (selected-path
            (if (and (pair? path) (integer? (car path)))
                path (cons index path)))
           (path~ ((self '~path-normalize) selected-path #f #f))
           (segments (cdr selected-path))
           (explicit? (eq? (car segments) '*bridge*))
           (name (if explicit? (cadr segments) (car segments)))
           (local-index (car path~))
           (local-chain
            ((self '~child) ((self '~field!) 'perm) 'previous index))
           (info ((standard 'deep-get) local-chain
                  `(,local-index (*bridge* ,name info))))
           (remote-chain ((standard 'deep-get) local-chain
                          `(,local-index (*bridge* ,name chain))))
           (remote (if (and (sync-node? remote-chain)
                            (not (equal? remote-chain '(nothing)))
                            (not (equal? remote-chain '(unknown)))) remote-chain
                       (error 'bridge-index-error
                              "Bridge is not committed at the selected local index: ~S ~S"
                              name local-index)))
           (remote-head-index (- ((self '~child) remote 'size) 1))
           (remote-index ((self '~child) remote 'index remote-index))
           (remote-path (if explicit? (cddr segments) (cdr segments))))
      `((interface ,(cadr (assoc 'interface info)))
        (remote-name ,(cadr (assoc 'remote-name info)))
        (public-key ,(cadr (assoc 'public-key info)))
        (head-index ,remote-head-index)
        (index ,remote-index)
        (path ,remote-path))))

  (define-method (~merge-head self path head (index -1))
    ;; Merge a fetched remote bridge head into the local chain head for a flat path.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (local-chain
            ((self '~child) ((self '~field!) 'perm) 'previous index))
           (remote-chain ((standard 'deep-get) local-chain (list (car path) (cadr path))))
           (remote-path (list-tail path 2))
           (remote-bridge? (and (> (length remote-path) 1)
                                (pair? (cadr remote-path))
                                (eq? (caadr remote-path) '*bridge*)
                                (> (length (cadr remote-path)) 1)))
           (prefix (reverse (list-tail (reverse path) (- (length path) 2)))))
      (if (and (not remote-bridge?)
               (not (equal? (sync-digest remote-chain) (sync-digest head))))
          (error 'integrity-error "Remote chain does not match local bridge head for path: ~S" path)
          ((standard 'deep-merge!) head local-chain prefix))))

  (define-method (~pin-prepare self path proof (head-node #f))
    ;; Anchor or select the immutable history head needed to pin one path.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (window ((self '~config-get) '(public window)))
           (current (- ((self 'size)) 1))
           (target ((self '~child) ((self '~field!) 'perm) 'index (car path))))
      (if (and window (<= target (- current window))) #f
          (let ((head-node
                 (or head-node
                     (if proof
                         (let* ((serialization
                                 (and (list? proof) (assoc 'proof proof)
                                      (cadr (assoc 'proof proof))))
                                (supplied-index
                                 (and (list? proof) (assoc 'index proof)
                                      (cadr (assoc 'index proof)))))
                           (if (not (and serialization (integer? supplied-index)))
                               (error 'integrity-error
                                      "Pinned proof requires an exact history index"))
                           ((self 'read)
                            ((self '~child) ((self '~field!) 'perm)
                             'index supplied-index)
                            ((standard 'deserialize) serialization)))
                         (let* ((head ((self '~head) path))
                                (node ((standard 'deep-get) head path)))
                           (if (or (equal? node '(nothing))
                                   (equal? node '(unknown)))
                               #f
                               ((standard 'deep-slice!) head path)))))))
            (and head-node (list path head-node))))))

  (define-method (~pin-apply! self prepared)
    ;; Merge one already anchored immutable history head into retention state.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (path (car prepared))
           (head-node (cadr prepared))
           (head-index ((self '~child) head-node 'index (car path)))
           (pinned
            ((standard 'deep-merge!)
             ((self '~child) head-node 'get (car path))
             ((self '~field!) 'perm) `(,head-index)))
           (temp ((self '~field!) 'temp))
           (temp-value ((standard 'deep-get) temp path))
           ;; A selected nested history entry can be absent from the temporary
           ;; origin head even while that origin index is recent.
           (historical-bridge-index?
            (let loop ((segments (cddr path)))
              (and (pair? segments)
                   (or (and (integer? (car segments)) (>= (car segments) 0))
                       (loop (cdr segments)))))))
      ((self '~field!) 'perm pinned)
      (if (and historical-bridge-index?
               (equal? temp-value '(unknown))
               (equal? (sync-digest pinned) (sync-digest temp)))
          ((self '~field!) 'temp
           ((standard 'deep-merge!)
            ((standard 'deep-slice!) pinned path) temp)))
      #t))

  (define-method (pin! self path proof)
    ;; Merge a prepared pinned proof object into the permanent chain.
    ;; A response is anchored against the matching retained local history root,
    ;; so a concurrent later step does not invalidate an already retrieved proof.
    (let ((prepared ((self '~pin-prepare) path proof)))
      (and prepared ((self '~pin-apply!) prepared))))

  (define-method (pin-batch! self paths (proofs #f))
    ;; Atomically retain ordered paths with optional prepared proof responses.
    ;; Internal prepared batches carry each route-group proof once plus an exact
    ;; slot vector; ordinary callers retain the aligned proof-list form. Exact
    ;; local no-proof duplicates reuse only a prior successful attempt member.
    (let* ((compact?
            (and (list? proofs) (= (length proofs) 2)
                 (list? (car proofs)) (= (length (car proofs)) 2)
                 (eq? (caar proofs) 'proofs)
                 (list? (cadr proofs)) (= (length (cadr proofs)) 2)
                 (eq? (caadr proofs) 'slots)))
           (groups (and compact? (cadar proofs)))
           (slots (and compact? (cadadr proofs))))
      (if (not (and (list? paths)
                    (or (not proofs)
                        (and compact? (list? groups) (list? slots)
                             (= (length slots) (length paths)))
                        (and (list? proofs)
                             (= (length proofs) (length paths))))))
          (error 'argument-error "Invalid pin batch shape"))
      (if compact?
          (let validate ((slots slots))
            (if (pair? slots)
                (let ((slot (car slots)))
                  (if (not (or (not slot)
                               (and (integer? slot) (>= slot 0)
                                    (< slot (length groups)))))
                      (error 'argument-error "Invalid prepared pin slot"))
                  (validate (cdr slots))))))
      (let ((perm ((self '~field!) 'perm))
            (temp ((self '~field!) 'temp)))
        (let loop ((paths paths)
                   (slots (if compact? slots
                              (if proofs proofs (map (lambda (path) #f) paths))))
                   (heads '())
                   (local-paths '()))
          (if (null? paths) #t
              (let* ((slot (car slots))
                     (proof (if compact? (and slot (list-ref groups slot)) slot))
                     (cached (and proof (assq proof heads)))
                     (local-duplicate
                      (and (not proof) (member (car paths) local-paths))))
                (cond
                 (local-duplicate
                  ;; An exact local path successfully retained earlier in this
                  ;; attempt needs no second immutable preparation or mutation.
                  (loop (cdr paths) (cdr slots) heads local-paths))
                 ((and compact? cached)
                  ;; The exact prepared slot references this already anchored
                  ;; route-group proof. Its union contains every selected path,
                  ;; so ordered duplicate and sibling slots require no repeated
                  ;; deserialize, anchor, or Merkle merge.
                  (loop (cdr paths) (cdr slots) heads local-paths))
                 (else
                  (let ((prepared
                         ((self '~pin-prepare) (car paths) proof
                          (and cached (cdr cached)))))
                    (if prepared
                        (begin
                          (if (not cached) ((self '~pin-apply!) prepared))
                          (loop
                           (cdr paths) (cdr slots)
                           (if (or (not proof) cached) heads
                               (cons (cons proof (cadr prepared)) heads))
                           (if proof local-paths
                               (cons (car paths) local-paths))))
                        (begin
                          ((self '~field!) 'perm perm)
                          ((self '~field!) 'temp temp)
                          #f)))))))))))

  (define-method (~retention-paths self paths)
    (if (not (list? paths)) (error 'argument-error "Batch paths must be a proper list"))
    (let* ((paths (map (lambda (path) ((self '~path-normalize) path #f #f)) paths))
           (protected?
            (lambda (path)
              (let ((last (and (pair? path) (car (reverse path)))))
                (or (not last) (integer? last)
                    (and (pair? last) (eq? (car last) '*bridge*))
                    (and (memq '*crypto*
                               (map (lambda (item) (and (pair? item) (car item))) path)) #t))))))
      (if (member #t (map protected? paths))
          (error 'path-error "Committed cryptographic material cannot be removed: ~S" paths))
      paths))

  (define-method (~retention-prune! self fields paths)
    (let* ((paths ((self '~retention-paths) paths))
           (standard (sync-eval ((self '~field!) 'standard)))
           (prune (lambda (node) (let loop ((node node) (paths paths))
                    (if (null? paths) node
                        (loop ((standard 'deep-prune!) node (car paths)) (cdr paths))))))
           (candidates (map (lambda (field) (prune ((self '~field!) field))) fields)))
      (for-each (lambda (field candidate) ((self '~field!) field candidate)) fields candidates)
      #t))

  (define-method (unpin! self path) ((self '~retention-prune!) '(perm) (list path)))

  (define-method (unpin-batch! self paths) ((self '~retention-prune!) '(perm) paths))

  (define-method (prune! self path) ((self '~retention-prune!) '(perm temp) (list path)))

  (define-method (prune-batch! self paths) ((self '~retention-prune!) '(perm temp) paths))

  (define-method (~signed-head self (known-index -1) (known-through -1))
    ;; Serialize the latest signed head and receiver-possible historical digests.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (perm ((self '~field!) 'perm))
           (rotation-indexes
            (if (< known-index 0) '()
                (let loop
                    ((index
                      ((self '~config-get)
                       '(public journal latest-rotation-index)))
                     (selected '()))
                  (if (or (not (integer? index)) (<= index known-index))
                      selected
                      (let* ((rotation
                              ((standard 'deep-get) perm
                               `(,index (*crypto* journal rotation))))
                             (previous
                              (and (list? rotation)
                                   (assoc 'previous-index rotation)
                                   (cadr (assoc 'previous-index rotation)))))
                        (if (not (and (integer? previous) (< previous index)))
                            (error 'integrity-error
                                   "Missing journal rotation at index: ~S"
                                   index))
                        (loop previous (cons index selected))))))))
      ((standard 'serialize) perm
       `(lambda (node)
          (let ((chain (sync-eval node)))
            (if (> ((chain 'size)) 0)
                (let* ((node ((chain 'get) -1))
                       (tree (sync-eval node)))
                  ((tree 'get) '(*crypto* public-key))
                  ((tree 'get) '(*crypto* signature))
                  ((tree 'get) '(*crypto* journal format-version))
                  ((tree 'get) '(*crypto* journal key-derivation-salt))
                  ((tree 'get) '(*crypto* journal identity id))
                  ((tree 'get) '(*crypto* journal identity nonce))
                  ((tree 'get) '(*crypto* journal latest-rotation-index))
                  ((tree 'get) '(*crypto* journal public-key))
                  ((tree 'get) '(*crypto* journal signature))
                  ((tree 'get) '(*crypto* interface public-key))
                  ((tree 'get) '(*crypto* interface endpoint))
                  ((tree 'get) '(*crypto* journal name))
                  ,@(map
                     (lambda (rotation-index)
                       `(let* ((rotation-node ((chain 'get) ,rotation-index))
                               (rotation-tree (sync-eval rotation-node)))
                          ((rotation-tree 'get) '(*crypto* journal rotation))
                          ((rotation-tree 'get) '(*crypto* journal format-version))
                          ((rotation-tree 'get) '(*crypto* journal identity id))
                          ((rotation-tree 'get) '(*crypto* journal identity nonce))))
                     rotation-indexes)
                  ((chain 'digest))
                  ,(if (>= known-index 0)
                       `(if ,(if (>= known-through known-index)
                                 #t `(< ,known-index ((chain 'size))))
                            (begin
                              ((chain 'digest) ,known-index)
                              ,(if (> known-through known-index)
                                   `((chain 'digest) ,known-through)
                                   #t))
                            #t)
                       #t))))))))

  (define-method (~trace self index path head (method #f) (arguments '()) (active? #f))
    ;; Trace a remote path against a serialized chain at index.
    (set! path ((self '~path-normalize) path #f #f))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (head (if head head ((self '~head) path index)))
           (last (and (pair? path) (car (reverse path))))
           (boundary? (or (null? path)
                          (and (pair? last) (= (length last) 3)
                               (eq? (car last) '*bridge*)
                               (eq? (caddr last) 'chain)))))
      ((standard 'serialize) head
       `(lambda (node)
          (letrec ((deep-get (lambda (node path)
                               (if (or (null? path)
                                       (equal? node '(nothing))
                                       (equal? node '(unknown))) node
                                   (let ((child (((sync-eval node) 'get) (car path))))
                                     (if (not (sync-node? child)) child
                                         (deep-get child (cdr path))))))))
            (let ((result (deep-get node ',path)))
              ,(cond
                (boundary?
                 '(let ((chain (sync-eval result)))
                    (if (member 'indices (chain '*api*)) ((chain 'indices))
                        (let ((size ((chain 'size))))
                          (let loop ((index 0))
                            (if (< index size)
                                (begin ((chain 'get) index) (loop (+ index 1)))))))
                    result))
                (active?
                 `(if (sync-node? result)
                      (let ((running (sync-eval result)))
                        ,(if (or (not method) (null? method))
                             '`((class ,(running '*name*))
                                (object-hash ,(sync-digest (running)))
                                (code-hash ,(sync-digest (sync-car (running)))))
                             `(apply (running ',method) ',arguments)))
                      ,(if (or method (pair? arguments))
                           '(error 'argument-error
                                   "Inert retrieve requires blank method and arguments")
                           'result)))
                ((or method (pair? arguments))
                 '(error 'argument-error
                         "Inert retrieve requires blank method and arguments"))
                (else 'result))))))))

  (define-method (~accept-peer-head! self name response info interface remote-name
                                      known-index verified)
    ;; Structurally apply a Federation-authenticated reciprocal peer head.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (latest ((standard 'deserialize) response))
           (existing ((self '~config-get) `(private bridge ,name)))
           (latest-size ((self '~child) latest 'size)))
      (if (= latest-size 0)
          (error 'bridge-sync-error
                 "A reciprocal bridge requires a signed peer head: ~S" name))
      (if (null? existing)
          ((self '~store-peer!) name verified))
      ((self '~store-peer-response!)
       name response interface remote-name verified)
      `((ok? #t)
        (accepted-index ,((self '~config-get) `(private bridge ,name last-index)))
        (head-index ,(- ((self 'size)) 1))
        (response ,((self '~signed-head) known-index))
        (info ,((self 'descriptor) -1))
        (interface ,((self '~config-get) '(public interface endpoint))))))

  (define-method (~store-peer-response! self name response interface remote-name verified)
    ;; Structurally validate and stage Federation-authenticated peer evidence.
    (if (and (list? response) (pair? response) (eq? (car response) 'error)) #f
        (let* ((standard (sync-eval ((self '~field!) 'standard)))
               (stage ((self '~field!) 'stage))
               (latest ((standard 'deserialize) response))
               (public-key (cadr (assoc 'public-key verified)))
               (value ((self '~child) stage 'get `(*bridge* ,name chain)))
               (latest-size ((self '~child) latest 'size))
               (last-size (if (sync-node? value) ((self '~child) value 'size) 0))
               (latest-index (if (> latest-size 0) (- latest-size 1) -1))
               (last-index (if (> last-size 0) (- last-size 1) -1)))
          (if (not (and (list? verified)
                        (byte-vector? public-key)
                        (= (cadr (assoc 'index verified)) latest-index)))
              (error 'bridge-sync-error
                     "Malformed authenticated peer evidence: ~S" name))
          (if (null? ((self '~config-get) `(private bridge ,name)))
              (error 'bridge-name-error "Unknown reciprocal bridge: ~S" name))
          (if (= latest-size 0)
              (error 'bridge-sync-error "A reciprocal bridge requires a signed peer head: ~S" name))
          ((self '~config-set!) `(private bridge ,name public-key) public-key)
          (if (< latest-index last-index)
              (error 'bridge-sync-error "Peer head is older than stored bridge head: ~S" name))
          (if (and (>= last-index 0)
                   (not (equal?
                         ((self '~child) value 'digest)
                         ((self '~child) latest 'digest last-index))))
              (error 'bridge-sync-error "Peer head does not continue stored bridge head: ~S" name))
          (if (= latest-index last-index) #t
              (let* ((stored-chain (if (> latest-size 0)
                                       ((standard 'deep-slice!) latest '(-1 ())) latest))
                     (info `((valid? #t)
                             (index ,latest-index)
                             (interface ,interface)
                             (public-key ,public-key)
                             (remote-name ,remote-name))))
                (set! stage
                      (sync-let ((stage stage) (name name)
                                 (latest-index latest-index) (info info)
                                 (stored-chain stored-chain))
                        (let ((stage (sync-eval stage)))
                          ((stage 'copy!) '(*transition*) '(*transition* previous))
                          ((stage 'set!) '(*transition* operation)
                           `((function synchronize!)
                             (path (*bridge* ,name))
                             (accepted-index ,latest-index)))
                          ((stage 'set!) `(*bridge* ,name info) info)
                          ((stage 'set!) `(*bridge* ,name chain) stored-chain)
                          (stage))))
                ((self '~config-set!) `(private bridge ,name last-index) latest-index)
                ((self '~field!) 'stage stage))))))

  (define-method (~step! self unix-time public-key secret-key)
    ;; Commit staged changes to permanent chain and update temp window.
    (let* ((window ((self '~config-get) '(public window)))
           (standard (sync-eval ((self '~field!) 'standard)))
           (stage ((self '~field!) 'stage))
           (perm ((self '~field!) 'perm))
           (temp ((self '~field!) 'temp))
           (perm-size ((self '~child) perm 'size))
           (pending-rotation
            ((self '~config-get) '(private journal pending-rotation)))
           (rotation-indexes
            ((self '~config-get) '(private journal rotation-indexes)))
           (configured-key ((self '~config-get) '(public journal public-key)))
           (key-change? (not (equal? configured-key public-key)))
           (curr-digest
            (sync-digest
             (cadr
              ((standard 'deep-call!) stage '()
               '(lambda (obj)
                  ((obj 'set!) '(*transition*) '(nothing)))))))
           (previous (and (> perm-size 0)
                          ((self '~child) perm 'get -1)))
           (prev-digest
            (cond ((= perm-size 0) (sync-digest (sync-null)))
                  ((equal? previous '(unknown)) #f)
                  (else
                   (sync-digest
                    (cadr
                     ((standard 'deep-call!)
                      previous
                      '()
                      '(lambda (obj)
                         ((obj 'set!) '(*transition*) '(nothing))
                         ((obj 'set!) '(*crypto*) '(nothing))))))))))
      (if key-change?
          (let ((expected-previous-index
                 (if (null? rotation-indexes) -1
                     (car (reverse rotation-indexes)))))
            (if (or (null? pending-rotation)
                    (not (= (cadr (assoc 'index pending-rotation))
                            perm-size))
                    (not (= (cadr (assoc 'previous-index pending-rotation))
                            expected-previous-index))
                    (not (equal? (cadr (assoc 'public-key pending-rotation))
                                 public-key)))
                (error 'key-rotation-error
                       "Root secret is not authorized by a pending key transition"))
            ((self '~rotation-verify) 2 #f pending-rotation configured-key))
          (if (not (null? pending-rotation))
              (error 'key-rotation-error
                     "Pending journal signing key does not match the root secret")))
      (if (or key-change? (not (equal? curr-digest prev-digest)))
          (let ((utc-time (system-time-utc unix-time)))
            (set! stage
                  (cadr
                   ((standard 'deep-call!) stage '()
                    `(lambda (tree)
                       ((tree 'copy!) '(*transition*) '(*transition* previous))
                       ((tree 'set!) '(*transition* operation)
                        '((path (*state* *time*)) (value ,utc-time)))
                       ((tree 'set!) '(*state* *time*) ,utc-time)))))
            (set! perm
                  (sync-let ((perm perm) (stage stage))
                    (let ((perm (sync-eval perm)))
                      ((perm 'push!) stage)
                      (perm))))
            (set! stage
                  (cadr
                   ((standard 'deep-call!) stage '()
                    '(lambda (tree) ((tree 'set!) '(*transition*) '(nothing))))))
            (set! perm
                  ((self '~signature-sign!)
                   perm public-key secret-key pending-rotation))
            ((self '~config-set!) '(public public-key) public-key)
            ((self '~config-set!) '(public journal public-key) public-key)
            (if key-change?
                (begin
                  ((self '~config-set!) '(private journal rotation-indexes)
                   (append rotation-indexes
                           (list (cadr (assoc 'index pending-rotation)))))
                  ((self '~config-set!) '(public journal latest-rotation-index)
                   (cadr (assoc 'index pending-rotation)))
                  ((self '~config-set!) '(private journal pending-rotation) '())))
            (let ((latest ((self '~child) perm 'get -1)))
              (set! temp
                    (sync-let ((temp temp) (latest latest))
                      (let ((temp (sync-eval temp)))
                        ((temp 'push!) latest)
                        (temp)))))
            (let ((time-node ((standard 'deep-slice!) temp '(-1 (*state* *time*)))))
              (if (and window (> ((self '~child) temp 'size) window))
                  (let ((truncate-index
                         (- ((self '~child) temp 'size) window 1)))
                    (set! temp
                          (cadr
                           ((standard 'deep-call!) temp '()
                            `(lambda (chain)
                               ((chain 'truncate!) ,truncate-index)))))))
              ((self '~field!) 'stage stage)
              ((self '~field!) 'perm
               ((standard 'deep-merge!) time-node
                ((standard 'deep-prune!) perm '(-1 (*state*)))))
              ((self '~field!) 'temp temp))))
      ((self '~child) perm 'size)))

  (define-method (~update-config! self path value)
    ;; Update a config entry in place. Shrinking the public retention window
    ;; destructively prunes newly excluded temporary history.
    (if (equal? path '(public window))
        (let* ((standard (sync-eval ((self '~field!) 'standard)))
               (temp ((self '~field!) 'temp))
               (old-window ((self '~config-get) '(public window)))
               (size ((self '~child) temp 'size)))
          (if (and (integer? value) (> size value)
                   (or (not (integer? old-window)) (< value old-window)))
              (set! temp
                    (cadr
                     ((standard 'deep-call!) temp '()
                      `(lambda (chain)
                         ((chain 'truncate!) ,(- size value 1)))))))
          ((self '~field!) 'temp temp)))
    ((self '~config-set!) path value))

  ;; Public data/history protocol used by Interface and Federation.

  (define-method (retrieve self path (pinned? #f) (proof? #f)
                          (head #f) (ancestor? #f) (method #f) (arguments '()))
    (let* ((normalized ((self '~path-normalize) path #f #f))
           (indexed-head?
            (and (list? head) (assoc 'index head) (assoc 'object head)))
           (head-index
            (if indexed-head? (cadr (assoc 'index head))
                ((self '~child) ((self '~field!) 'perm)
                 'index (car normalized))))
           (head-object
            (if indexed-head? (cadr (assoc 'object head)) head)))
      ((self '~retrieve) path pinned? proof?
       (and head-object ((self '~anchor) head-index head-object)) ancestor?
       method arguments)))

  (define-method (trace self path (head #f))
    (if (not head)
        ((self '~trace)
         (if (and (pair? path) (integer? (car path))) (car path) -1)
         (if (and (pair? path) (integer? (car path))) (cdr path) path)
         #f #f '())
        (let* ((standard (sync-eval ((self '~field!) 'standard)))
               (normalized ((self '~path-normalize) path #f #f)))
          ((standard 'serialize)
           ((standard 'deep-slice!) head normalized)))))

  (define-method (trace-batch self paths (head #f))
    ;; Serialize the union of accesses for paths sharing one authenticated head.
    (if (not (and (list? paths) (pair? paths)))
        (error 'argument-error "Trace batch requires at least one path"))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (normalized (map (lambda (path)
                              ((self '~path-normalize) path #f #f))
                            paths))
           (trace-items
            (map
             (lambda (path)
               (let ((last (and (pair? path) (car (reverse path)))))
                 (list
                  path
                  (or (null? path)
                      (and (pair? last) (= (length last) 3)
                           (eq? (car last) '*bridge*)
                           (eq? (caddr last) 'chain))))))
             normalized))
           (indexes (map car normalized))
           (index (car indexes)))
      (if (not (let loop ((indexes (cdr indexes)))
                 (or (null? indexes)
                     (and (= (car indexes) index)
                          (loop (cdr indexes))))))
          (error 'index-error "Trace batch paths require one history anchor"))
      (let ((head
             (if head head
                 (let loop ((paths normalized) (proof #f))
                   (if (null? paths) proof
                       (let* ((source ((self '~head) (car paths)))
                              (slice
                               ((standard 'deep-slice!) source (car paths))))
                         (loop
                          (cdr paths)
                          (if proof
                              ((standard 'deep-merge!) slice proof)
                              slice))))))))
        ((standard 'serialize) head
         `(lambda (node)
            (letrec ((deep-get
                      (lambda (node path)
                        (if (or (null? path)
                                (equal? node '(nothing))
                                (equal? node '(unknown))) node
                            (let ((child (((sync-eval node) 'get) (car path))))
                              (if (not (sync-node? child)) child
                                  (deep-get child (cdr path))))))))
              (for-each
               (lambda (item)
                 (let ((result (deep-get node (car item))))
                   (if (cadr item) (sync-car result) result)))
               ',trace-items)))))))

  (define-method (pinned? self path)
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (normalized ((self '~path-normalize) path #f #f)))
      (not (equal? ((standard 'deep-get)
                    ((self '~field!) 'perm) normalized)
                   '(unknown)))))

  (define-method (signed-head self known-index)
    (cond ((integer? known-index) ((self '~signed-head) known-index -1))
          ((and (list? known-index) (assoc 'index known-index))
           ((self '~signed-head) (cadr (assoc 'index known-index))
            (if (assoc 'through known-index)
                (cadr (assoc 'through known-index)) -1)))
          (else (error 'index-error "Invalid signed-head cursor"))))

  (define-method (peer-head self alias index)
    (if (and (symbol? alias) (not ((self '~name-admissible?) alias)))
        (error 'bridge-name-error "Bridge alias does not round trip through the expression codec: ~S" alias))
    (if (and (symbol? alias) (list? index) (assoc 'checkpoint index))
        ((self '~peer-checkpoint) alias)
        (let* ((path (if (list? alias) alias `(*bridge* ,alias)))
               (local-index
                (if (and (list? index) (assoc 'local index))
                    (cadr (assoc 'local index)) index))
               (remote-index
                (if (and (list? index) (assoc 'remote index))
                    (cadr (assoc 'remote index)) -1)))
          ((self '~peer-head) path local-index remote-index))))

  (define-method (~peer-checkpoint self alias)
    ;; Return inert alias-scoped accepted-key evidence and an opaque apply precondition.
    (let* ((active ((self '~config-get) `(private bridge ,alias)))
           (retired ((self '~config-get) `(private bridge-retired ,alias)))
           (preapproval ((self '~config-get) `(private bridge-preapproval ,alias)))
           (selected (if (null? active) retired active))
           (get (lambda* (key (default '()))
                  (let ((entry (and (list? selected) (assoc key selected))))
                    (if entry (cadr entry) default))))
           (status (cond ((not (null? active)) 'active)
                         ((not (null? retired)) 'retired) (else 'absent)))
           (head ((self '~child) ((self '~field!) 'stage) 'get `(*bridge* ,alias chain)))
           (head-digest (if (sync-node? head) (sync-digest head) '()))
           (body `((status ,status) (public-key ,(get 'public-key))
                   (accepted-index ,(get 'last-index -1)) (head-digest ,head-digest)
                   (acceptance ,((self '~config-get) '(public bridge-accept)))
                   (preapproval ,preapproval))))
      (append body `((checkpoint ,(sync-hash (expression->byte-vector body)))))))

  (define-method (merge-head! self alias supplied-head)
    (let* ((path (cond ((and (list? alias) (assoc 'path alias))
                        (cadr (assoc 'path alias)))
                       ((list? alias) alias)
                       (else `(*bridge* ,alias))))
           (head (if (and (list? supplied-head) (assoc 'head supplied-head))
                     (cadr (assoc 'head supplied-head)) supplied-head))
           (index (cond ((and (list? alias) (assoc 'index alias))
                         (cadr (assoc 'index alias)))
                        ((and (list? supplied-head) (assoc 'index supplied-head))
                         (cadr (assoc 'index supplied-head)))
                        (else -1))))
      ((self '~merge-head) path head index)))

  (define-method (store-peer-head! self alias verified-head)
    (if (not (and (symbol? alias) ((self '~name-admissible?) alias)
                  (list? verified-head)
                  (assoc 'mode verified-head)))
        (error 'bridge-sync-error
               "Malformed prepared peer-head update: ~S" verified-head))
    (if (and (assoc 'checkpoint verified-head)
             (not (equal?
                   (cadr (assoc 'checkpoint verified-head))
                   (cadr (assoc 'checkpoint
                                ((self '~peer-checkpoint) alias))))))
        (error 'federation-conflict
               "Peer checkpoint changed before durable apply: ~S" alias))
    (case (cadr (assoc 'mode verified-head))
      ((accept)
       ((self '~accept-peer-head!)
        alias
        (cadr (assoc 'response verified-head))
        (and (assoc 'info verified-head)
             (cadr (assoc 'info verified-head)))
        (cadr (assoc 'interface verified-head))
        (cadr (assoc 'remote-name verified-head))
        (if (assoc 'known-index verified-head)
            (cadr (assoc 'known-index verified-head)) -1)
        (cadr (assoc 'verified verified-head))))
      ((update)
       ((self '~store-peer-response!)
        alias (cadr (assoc 'response verified-head))
        (cadr (assoc 'interface verified-head))
        (cadr (assoc 'remote-name verified-head))
        (cadr (assoc 'verified verified-head))))
      ((establish)
       ((self '~store-peer!) alias (cadr (assoc 'verified verified-head)))
       ((self '~store-peer-response!)
        alias
        (cadr (assoc 'response verified-head))
        (cadr (assoc 'interface verified-head))
        (cadr (assoc 'remote-name verified-head))
        (cadr (assoc 'verified verified-head))))
      (else
       (error 'bridge-sync-error
              "Unknown prepared peer-head update mode: ~S"
              (cadr (assoc 'mode verified-head))))))

  (define-method (step! self prepared-inputs)
    (if (not (and (list? prepared-inputs)
                  (assoc 'unix-time prepared-inputs)
                  (assoc 'public-key prepared-inputs)
                  (assoc 'secret-key prepared-inputs)))
        (error 'argument-error "Malformed prepared Ledger step: ~S"
               prepared-inputs))
    (if (assoc 'rotation prepared-inputs)
        (let ((rotation (cadr (assoc 'rotation prepared-inputs))))
          ((self '~rotate-key!)
           (cadr (assoc 'previous-key rotation))
           (cadr (assoc 'public-key rotation))
           (cadr (assoc 'signature rotation)))))
    ((self '~step!)
     (cadr (assoc 'unix-time prepared-inputs))
     (cadr (assoc 'public-key prepared-inputs))
     (cadr (assoc 'secret-key prepared-inputs))))

  (define-method (update-config! self changes)
    (if (and (equal? (cadr (assoc 'path changes)) '(public window))
             (not (or (not (cadr (assoc 'value changes)))
                      (and (integer? (cadr (assoc 'value changes)))
                           (> (cadr (assoc 'value changes)) 0)))))
        (error 'argument-error "Window must be positive"))
    (if (not (and (list? changes) (assoc 'path changes)
                  (assoc 'value changes)))
        (error 'argument-error "Malformed Ledger config change: ~S" changes))
    ((self '~update-config!)
     (cadr (assoc 'path changes)) (cadr (assoc 'value changes))))

  (define-method (~field! self name value)
    ;; Retrieve or set internal field by name.
    (let ((address (case name
                     ((standard) '(1 0 0 0))
                     ((config) '(1 0 0 1))
                     ((stage) '(1 0 1 0))
                     ((temp) '(1 0 1 1))
                     ((perm) '(1 1))
                     (else (error 'field-error "Ledger field not found: ~S" name)))))
      (if value (set! (self address) value)
          (self address))))

  (define-method (~path-normalize self path stage? state-only?)
    ;; Convert a flat public path into the current nested ledger representation.
    (define (reject reason)
      (error 'path-error "Invalid ledger path (~A): ~S" reason path))
    (define (contains-pair? xs)
      (and (pair? xs)
           (or (pair? (car xs)) (contains-pair? (cdr xs)))))
    (define (admit-name name)
      (if (and (symbol? name) (not ((self '~name-admissible?) name)))
          (reject "symbol does not round trip through the expression codec"))
      name)
    (define (admit-state segments)
      (for-each admit-name segments)
      segments)
    (define (indexed-tail segments)
      (cond ((null? segments) '())
            ((integer? (car segments)) (indexed-segments (car segments) (cdr segments)))
            (else (indexed-segments -1 segments))))
    (define (indexed-segments index segments)
      (cond ((null? segments) `(,index))
            ((eq? (car segments) '*state*) `(,index ,(cons '*state* (admit-state (cdr segments)))))
            ((eq? (car segments) '*transition*) `(,index ,segments))
            ((eq? (car segments) '*crypto*) `(,index ,segments))
            ;; `*bridge*` remains the local bridge namespace and the explicit
            ;; compatibility form. Concise paths name each bridge directly and
            ;; optionally follow it with the selected remote index.
            ((eq? (car segments) '*bridge*)
             (cond ((null? (cdr segments)) `(,index (*bridge*)))
                   ((null? (cddr segments)) `(,index (*bridge* ,(admit-name (cadr segments)) chain)))
                   (else (append `(,index (*bridge* ,(admit-name (cadr segments)) chain))
                                 (indexed-tail (cddr segments))))))
            ((symbol? (car segments))
             (let* ((name (admit-name (car segments)))
                    (rest (cdr segments))
                    (remote-index (if (and (pair? rest) (integer? (car rest)))
                                      (car rest) -1))
                    (tail (if (and (pair? rest) (integer? (car rest)))
                              (cdr rest) rest))
                    (edge `(,index (*bridge* ,name chain))))
               (if (null? rest) edge
                   (append edge (indexed-segments remote-index tail)))))
            (else (reject "expected bridge name or namespace marker"))))
    (cond ((not (list? path)) (reject "not a list"))
          ((contains-pair? path) (reject "nested public paths are not supported"))
          (stage?
           (cond ((or (null? path) (integer? (car path))) (reject "stage paths cannot start with an index"))
                 ((and state-only? (not (eq? (car path) '*state*))) (reject "expected *state*"))
                 ((eq? (car path) '*state*) `(,(cons '*state* (admit-state (cdr path)))))
                 ((and (not state-only?) (eq? (car path) '*transition*)) `(,path))
                 ((and (not state-only?) (eq? (car path) '*bridge*))
                  (cond ((null? (cdr path)) '((*bridge*)))
                        ((null? (cddr path)) `((*bridge* ,(admit-name (cadr path)) chain)))
                        (else (reject "stage bridge traversal is not supported"))))
                 (else (reject "expected namespace marker"))))
          (else (indexed-tail path))))

  (define-method (~name-admissible? self name)
    ;; Naming symbols must survive the exact durable expression codec canonically.
    (and (symbol? name)
         (let* ((encoded (expression->byte-vector name))
                (decoded (byte-vector->expression encoded)))
           (and (symbol? decoded)
                (equal? (symbol->string decoded) (symbol->string name))
                (equal? encoded (expression->byte-vector decoded))))))

  (define-method (delete-peer-head! self alias)
    ;; Remove all active/retired alias continuity while preserving route-shaped grants.
    (if (not ((self '~name-admissible?) alias))
        (error 'bridge-name-error "Bridge alias does not round trip through the expression codec: ~S" alias))
    ((self '~config-set!) `(private bridge ,alias) '())
    ((self '~config-set!) `(private bridge-retired ,alias) '())
    ((self '~field!) 'stage
     (cadr
      (((sync-eval ((self '~field!) 'standard)) 'deep-call!)
       ((self '~field!) 'stage) '()
       `(lambda (tree) ((tree 'set!) '(*bridge* ,alias) '(nothing)))))))

  (define-method (~config-get self (path '()))
    (let loop ((config (byte-vector->expression ((self '~field!) 'config))) (path path))
      (if (null? path) config
          (let ((match (assoc (car path) config)))
            (if (not match) '()
                (loop (cadr match) (cdr path)))))))

  (define-method (~config-set! self (path '()) value)
    ;; Alias-keyed config tables bypass Tree, so enforce the same exact symbol
    ;; admission before serializing their durable expression keys.
    (if (and (list? path) (>= (length path) 3)
             (eq? (car path) 'private)
             (memq (cadr path) '(bridge bridge-retired bridge-preapproval))
             (not ((self '~name-admissible?) (caddr path))))
        (error 'bridge-name-error
               "Bridge alias does not round trip through the expression codec: ~S"
               (caddr path)))
    ((self '~field!) 'config
     (expression->byte-vector
      (let loop-1 ((config (byte-vector->expression ((self '~field!) 'config))) (path path))
        (if (null? path) value
            (let loop-2 ((config config))
              (cond ((null? config)
                     (if (eq? value '()) '()
                         (list (list (car path) (loop-1 '() (cdr path))))))
                    ((eq? (caar config) (car path))
                     (let ((result (loop-1 (cadar config) (cdr path))))
                       (if (eq? result '()) (cdr config)
                           (cons (list (car path) result) (cdr config)))))
                    (else (cons (car config) (loop-2 (cdr config)))))))))))

  (define-method (~head self path (index -1))
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (window ((self '~config-get) '(public window)))
           (selected (cond ((null? path) -1) ((>= (car path) 0) (car path)) (else (+ index 1 (car path)))))
           (perm-info
            ((standard 'deep-call) ((self '~field!) 'perm) '()
             `(lambda (chain) (list ((chain 'index) ,selected)
                                    (- ((chain 'size)) 1) ((chain 'previous) ,index))))))
      (if (and window (<= (car perm-info) (- (cadr perm-info) window))) (caddr perm-info)
          (let ((temp-head ((self '~child) ((self '~field!) 'temp) 'previous index)))
            (if (and (equal? ((standard 'deep-get) temp-head path) '(unknown))
                     (equal? (sync-digest (caddr perm-info))
                             (sync-digest temp-head)))
                (caddr perm-info) temp-head)))))

  (define-method (~signature-sign! self chain public-key secret-key rotation)
    ;; Embed versioned Journal recovery material and signatures into the new head.
    (let* ((standard (sync-eval ((self '~field!) 'standard)))
           (salt ((self '~config-get) '(public key-derivation-salt)))
           (latest-rotation-index
            (if (null? rotation) ((self '~config-get) '(public journal latest-rotation-index))
                (cadr (assoc 'index rotation))))
           (interface-public-key ((self '~config-get) '(public interface public-key)))
           (interface-endpoint ((self '~config-get) '(public interface endpoint)))
           (journal-name ((self '~config-get) '(public name))))
      (if (not (and (byte-vector? salt) (= (length salt) 32)))
          (error 'integrity-error "Journal key-derivation salt is invalid"))
      (if (not (null? interface-endpoint))
          (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* interface endpoint)) interface-endpoint)))
      (if (not (null? journal-name))
          (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal name)) journal-name)))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* public-key)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* signature)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal format-version)) 2))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal key-derivation-salt)) salt))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal latest-rotation-index)) latest-rotation-index))
      (if (not (null? rotation))
          (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal rotation)) rotation)))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal public-key)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* journal signature)) #u()))
      (set! chain ((standard 'deep-set!) chain '(-1 (*crypto* interface public-key)) interface-public-key))
      (let ((signature (crypto-sign secret-key (sync-digest chain))))
        (cadr
         ((standard 'deep-call!) chain '(-1)
          `(lambda (tree)
             ((tree 'set!) '(*crypto* public-key) ,public-key)
             ((tree 'set!) '(*crypto* signature) ,signature)
             ((tree 'set!) '(*crypto* journal public-key) ,public-key)
             ((tree 'set!) '(*crypto* journal signature) ,signature)))))))

)
