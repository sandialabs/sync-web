(define-class (authorization)
  ;; Authorization policy object for interface-level request authorization.
  (define-method (*init* self)
    ;; Initialize authorization state.
    ;;   Returns:
    ;;     boolean: #t after storing an empty local policy table.
    (set! (self '(1)) (expression->byte-vector '())))

  (define-method (authorizations self (principal #f))
    ;; Return the complete policy table or one local owner's rules.
    (if (not principal) ((self '~policies))
        (begin
          (if (not ((self '~local-user-principal?) principal))
              (error 'authorization-error
                     "Authorization rules belong to local user principals: ~S"
                     principal))
          ((self '~user-rules) principal))))

  (define-method (authorize! self rule)
    ;; Add one normalized rule from an Interface-owned administration envelope.
    (if (not (and (list? rule) (= (length rule) 3)
                  (assoc 'user rule) (assoc 'rule rule)
                  (assoc 'latest-index rule)))
        (error 'authorization-error "Malformed authorization envelope: ~S" rule))
    (let ((user (cadr (assoc 'user rule)))
          (latest-index (cadr (assoc 'latest-index rule)))
          (candidate (cadr (assoc 'rule rule))))
      (if (not ((self '~local-user-principal?) user))
          (error 'authorization-error
                 "Authorization rules belong to local user principals: ~S" user))
      (let* ((normalized ((self '~normalize-rule) candidate latest-index))
             (rules ((self '~user-rules) user)))
        ((self '~set-user-rules!)
         user (if (member normalized rules) rules (cons normalized rules))))))

  (define-method (deauthorize! self rule-or-selector)
    ;; Remove one rule or every rule under a bridge-principal prefix.
    (cond
     ((and (list? rule-or-selector) (= (length rule-or-selector) 1)
           (assoc 'principal-prefix rule-or-selector))
      (let ((prefix (cadr (assoc 'principal-prefix rule-or-selector))))
        (if (not (and (list? prefix) (= (length prefix) 1)
                      (symbol? (car prefix))))
            (error 'authorization-error
                   "Invalid principal-prefix selector: ~S" prefix))
        ((self '~set-policies!)
         (let loop-users ((policies ((self '~policies))) (out '()))
           (if (null? policies) (reverse out)
               (let* ((user (caar policies))
                      (rules
                       (let loop-rules ((rules (cadar policies)) (kept '()))
                         (cond ((null? rules) (reverse kept))
                               (((self '~bridge-principal?)
                                 (cadr (assoc 'principal (car rules)))
                                 (car prefix))
                                (loop-rules (cdr rules) kept))
                               (else
                                (loop-rules (cdr rules)
                                            (cons (car rules) kept)))))))
                 (loop-users
                  (cdr policies)
                  (if (null? rules) out (cons (list user rules) out)))))))
        #t))
     ((and (list? rule-or-selector) (= (length rule-or-selector) 2)
           (assoc 'user rule-or-selector) (assoc 'rule rule-or-selector))
      (let ((user (cadr (assoc 'user rule-or-selector)))
            (candidate (cadr (assoc 'rule rule-or-selector))))
        (if (not ((self '~local-user-principal?) user))
            (error 'authorization-error
                   "Authorization rules belong to local user principals: ~S" user))
        (let ((normalized ((self '~normalize-rule) candidate #f)))
          ((self '~set-user-rules!)
           user
           (let loop ((rules ((self '~user-rules) user)) (out '()))
             (cond ((null? rules) (reverse out))
                   ((equal? normalized (car rules)) (loop (cdr rules) out))
                   (else (loop (cdr rules) (cons (car rules) out)))))))))
     (else
      (error 'authorization-error
             "Malformed deauthorization envelope or selector: ~S"
             rule-or-selector))))

  (define-method (authorized? self principal key-index path operation)
    ;; Classify a canonical request as direct, ancestor-admitted, or denied.
    (set! principal ((self '~normalize-principal) principal))
    (if (not (and (list? key-index) (assoc 'latest-index key-index)))
        (error 'authorization-error
               "Authorization context lacks latest-index: ~S" key-index))
    (let* ((request-operation operation)
           (operation (if (eq? operation 'trace) 'resolve operation))
           (latest-index (cadr (assoc 'latest-index key-index)))
           (args `((path ,path)))
           (info ((self '~request-target) operation args latest-index))
           (owner (cadr (assoc 'owner info))))
      (cond
       (((self '~bridge-path-request?) request-operation args)
        'direct)
       ((and owner
             (or (equal? principal owner)
                 ((self '~rule-authorized?)
                  owner principal info latest-index key-index)))
        'direct)
       ((and (memq operation '(get resolve))
             (or
              (and owner
                   (let loop ((rules ((self '~user-rules) owner)))
                     (and (pair? rules)
                          (or
                           (let* ((rule (car rules))
                                  (rule-principal
                                   ((self '~normalize-principal)
                                    (cadr (assoc 'principal rule))))
                                  (rule-path (cadr (assoc 'path rule)))
                                  (rest ((self '~path-suffix)
                                         (cadr (assoc 'relative-path info))
                                         rule-path))
                                  (index (cadr (assoc 'index info))))
                             (and (pair? rest)
                                  (or (equal? principal rule-principal)
                                      (equal? rule-principal '(*public*)))
                                  (or (equal? rule-principal '(*public*))
                                      ((self '~key-index-authorized?)
                                       rule key-index))
                                  (case operation
                                    ((get) (cadr (assoc 'get rule)))
                                    ((resolve)
                                     ((self '~resolve-authorized?)
                                      (cadr (assoc 'resolve rule))
                                      index latest-index))
                                    (else #f))))
                           (loop (cdr rules))))))
              (and ((self '~state-root-request?) args)
                   ((self '~state-root-authorized?)
                    principal operation args latest-index key-index))))
        'ancestor)
       (else #f))))

  (define-method (~state-root-request? self args)
    ;; Return #t when a request addresses the user-namespace directory itself.
    (let ((path-entry (assoc 'path args)))
      (and path-entry
           (let ((path (cadr path-entry)))
             (and (list? path)
                  (equal? (if (and (pair? path) (integer? (car path))) (cdr path) path)
                          '(*state*)))))))

  (define-method (~state-root-authorized? self principal operation args latest-index authentication-context)
    ;; Admit namespace-root traversal when this principal owns or has a
    ;; descendant grant beneath at least one user namespace. The returned
    ;; directory remains unfiltered apart from reserved names; opening each
    ;; user folder is authorized independently.
    (let* ((path (cadr (assoc 'path args)))
           (explicit-index (and (pair? path) (integer? (car path)) (car path)))
           (index (and explicit-index ((self '~resolve-index) explicit-index latest-index))))
      (or ((self '~local-user-principal?) principal)
          (let loop-users ((policies ((self '~policies))))
            (and (pair? policies)
                 (or (let loop-rules ((rules (cadar policies)))
                       (and (pair? rules)
                            (or (let* ((rule (car rules))
                                       (rule-principal
                                        ((self '~normalize-principal)
                                         (cadr (assoc 'principal rule)))))
                                  (and (or (equal? principal rule-principal)
                                           (equal? rule-principal '(*public*)))
                                       (or (equal? rule-principal '(*public*))
                                           ((self '~key-index-authorized?)
                                            rule authentication-context))
                                       (case operation
                                         ((get) (cadr (assoc 'get rule)))
                                         ((resolve)
                                          ((self '~resolve-authorized?)
                                           (cadr (assoc 'resolve rule)) index latest-index))
                                         (else #f))))
                                (loop-rules (cdr rules)))))
                     (loop-users (cdr policies))))))))

  (define-method (~policies self)
    ;; Return the local policy table.
    (byte-vector->expression (self '(1))))

  (define-method (~set-policies! self policies)
    ;; Store the local policy table.
    (set! (self '(1)) (expression->byte-vector policies)))

  (define-method (~user-rules self user)
    ;; Return rules for a local user principal.
    (let ((entry (assoc user ((self '~policies)))))
      (if entry (cadr entry) '())))

  (define-method (~set-user-rules! self user rules)
    ;; Replace rules for a local user principal.
    ((self '~set-policies!)
     (let loop ((policies ((self '~policies))) (out '()) (updated? #f))
       (cond ((null? policies)
              (reverse (if updated? out (cons (list user rules) out))))
             ((equal? user (caar policies))
              (loop (cdr policies) (cons (list user rules) out) #t))
             (else (loop (cdr policies) (cons (car policies) out) updated?)))))
    #t)

  (define-method (~normalize-rule self rule latest-index)
    ;; Validate and normalize a rule into canonical field order.
    (if (not (list? rule))
        (error 'authorization-error "Authorization rule must be an alist: ~S" rule))
    (let ((allowed '(principal key-index path get set! resolve)))
      (let loop ((entries rule) (seen '()))
        (if (pair? entries)
            (let ((entry (car entries)))
              (if (not (and (list? entry) (= (length entry) 2)
                            (symbol? (car entry))
                            (memq (car entry) allowed)
                            (not (memq (car entry) seen))))
                  (error 'authorization-error
                         "Authorization rule has an unknown, duplicate, or malformed field: ~S"
                         entry))
              (loop (cdr entries) (cons (car entry) seen))))))
    (if (or (not (assoc 'principal rule)) (not (assoc 'path rule)))
        (error 'authorization-error
               "Authorization rule requires principal and path fields: ~S" rule))
    (let* ((principal ((self '~normalize-principal)
                       (cadr (assoc 'principal rule))))
           (path (cadr (assoc 'path rule)))
           (get (if (assoc 'get rule) (cadr (assoc 'get rule)) #f))
           (set! (if (assoc 'set! rule) (cadr (assoc 'set! rule)) #f))
           (resolve (if (assoc 'resolve rule) (cadr (assoc 'resolve rule)) #f))
           (key-index (and (assoc 'key-index rule) (cadr (assoc 'key-index rule)))))
      (if (not ((self '~principal?) principal))
          (error 'authorization-error "Invalid authorization principal: ~S" principal))
      (if (and (not (equal? principal '(*public*)))
               (not ((self '~local-user-principal?) principal))
               (not key-index))
          (error 'authorization-error
                 "Remote authorization principals require a key-index window"))
      (if (not (and (list? path) (not ((self '~contains-pair?) path))))
          (error 'authorization-error "Authorization path must be a flat relative path: ~S" path))
      (for-each (lambda (entry)
                  (if (not (boolean? (cadr entry)))
                      (error 'authorization-error "Authorization flag must be boolean: ~S" entry)))
                `((get ,get) (set! ,set!)))
      (if (not ((self '~resolve-range?) resolve latest-index))
          (error 'authorization-error "Invalid resolve authorization: ~S" resolve))
      (if (and key-index (not ((self '~resolve-range?) key-index #f)))
          (error 'authorization-error "Invalid terminal authentication index authorization: ~S" key-index))
      `((principal ,principal)
        ,@(if key-index `((key-index ,key-index)) '())
        (path ,path)
        (get ,get)
        (set! ,set!)
        (resolve ,resolve))))

  (define-method (~rule-authorized? self owner principal info latest-index authentication-context)
    ;; Return #t if any stored rule authorizes the request target and signing-key window.
    (let ((relative-path (cadr (assoc 'relative-path info)))
          (permission (cadr (assoc 'permission info)))
          (index (cadr (assoc 'index info))))
      (let loop ((rules ((self '~user-rules) owner)))
        (and (pair? rules)
             (or (let* ((rule (car rules))
                        (rule-principal
                         ((self '~normalize-principal)
                          (cadr (assoc 'principal rule)))))
                   (and (or (equal? principal rule-principal)
                            (equal? rule-principal '(*public*)))
                        ((self '~path-prefix?) (cadr (assoc 'path rule)) relative-path)
                        (or (equal? rule-principal '(*public*))
                            ((self '~key-index-authorized?) rule authentication-context))
                        (case permission
                          ((get) (cadr (assoc 'get rule)))
                          ((set!) (cadr (assoc 'set! rule)))
                          ((resolve) ((self '~resolve-authorized?) (cadr (assoc 'resolve rule)) index latest-index))
                          (else #f))))
                 (loop (cdr rules)))))))

  (define-method (~key-index-authorized? self rule authentication-context)
    ;; Check which local committed head authenticated a remote principal.
    (if (not (assoc 'authentication-index authentication-context))
        (not (assoc 'key-index rule))
        (let ((permission (and (assoc 'key-index rule) (cadr (assoc 'key-index rule))))
              (latest-index (cadr (assoc 'latest-index authentication-context)))
              (authentication-index (cadr (assoc 'authentication-index authentication-context))))
          (and permission
               (let ((start ((self '~resolve-index) (car permission) latest-index))
                     (end ((self '~resolve-index) (cadr permission) latest-index)))
                 (and (<= start authentication-index) (<= authentication-index end)))))))

  (define-method (~request-target self operation args latest-index)
    ;; Extract owner, owner-relative path, permission, and normalized index for a request.
    (let* ((path-entry (assoc 'path args))
           (raw-path (and path-entry (cadr path-entry))))
      (case operation
        ((get set!)
         ((self '~path-target) raw-path operation #f latest-index))
        ((pin! unpin!)
         ((self '~path-target)
          ((self '~retention-state-path) raw-path) operation #f latest-index))
        ((resolve)
         ((self '~path-target) raw-path 'resolve #t latest-index))
        ((trace)
         (let ((index (and (assoc 'index args) (cadr (assoc 'index args)))))
           ((self '~path-target) raw-path 'resolve index latest-index)))
        (else '((owner #f))))))

  (define-method (~explicit-bridge-path self path)
    ;; Expand concise `name [index] ... namespace ...` traversal into the
    ;; original explicit `*bridge* name [index] ...` representation used by
    ;; authorization helpers. Explicit paths remain accepted unchanged.
    (if (or (not (list? path)) (member '*bridge* path)) path
        (let loop ((rest path) (out '()))
          (cond ((null? rest) (reverse out))
                ((or (integer? (car rest))
                     (memq (car rest) '(*state* *transition* *crypto*)))
                 (if (and (symbol? (car rest))
                          (memq (car rest) '(*state* *transition* *crypto*)))
                     (append (reverse out) rest)
                     (loop (cdr rest) (cons (car rest) out))))
                ((symbol? (car rest))
                 (loop (cdr rest) (cons (car rest) (cons '*bridge* out))))
                (else path)))))

  (define-method (~bridge-path-request? self operation args)
    ;; Expose only bridge directories, exact object boundaries, and crypto tails.
    (and (memq operation '(get resolve trace))
         (let* ((raw (and (assoc 'path args) (cadr (assoc 'path args))))
                (path (and (list? raw) ((self '~explicit-bridge-path) raw)))
                (root (if (and (pair? path) (integer? (car path)))
                          (cdr path) path)))
           (and path
                (or (and (eq? operation 'trace)
                         (or (null? root) (eq? (car root) '*crypto*)))
                    ((self '~public-bridge-path?) path operation))))))

  (define-method (~public-bridge-path? self path operation)
    (let loop ((path ((self '~explicit-bridge-path)
                      (if (and (pair? path) (integer? (car path))) (cdr path) path))))
      (cond ((equal? path '(*bridge*)) (not (not (memq operation '(get resolve)))))
            ((not (and (pair? path) (eq? (car path) '*bridge*)
                       (pair? (cdr path)) (symbol? (cadr path)))) #f)
            (else
             (let ((rest (cddr path)))
               (if (and (pair? rest) (integer? (car rest))) (set! rest (cdr rest)))
               (cond ((null? rest) (eq? operation 'trace))
                     ((eq? (car rest) '*bridge*) (loop rest))
                     ((eq? (car rest) '*crypto*)
                      (not (not (memq operation '(get resolve trace)))))
                     (else #f)))))))

  (define-method (~retention-state-path self path)
    ;; Return the terminal state path from a local retained-history path.
    ;; A retained federated proof is addressed relative to Self through one or
    ;; more indexed bridge objects, but ownership belongs to its terminal
    ;; `*state*` path. Malformed or non-state retention paths remain ownerless.
    (let loop ((rest ((self '~explicit-bridge-path) path)))
      (cond ((and (pair? rest) (integer? (car rest))) (loop (cdr rest)))
            ((and (pair? rest) (eq? (car rest) '*bridge*)
                  (pair? (cdr rest)) (symbol? (cadr rest)))
             (loop (cddr rest)))
            ((and (pair? rest) (eq? (car rest) '*state*)) rest)
            (else #f))))

  (define-method (~path-target self raw-path permission index latest-index)
    ;; Convert a public path into authorization target metadata.
    (if (not (list? raw-path)) '((owner #f))
        (let* ((explicit-index (and (pair? raw-path) (integer? (car raw-path)) (car raw-path)))
               (path (if explicit-index (cdr raw-path) raw-path))
               (request-index (cond ((eq? index #f) #f)
                                    ((eq? index #t) explicit-index)
                                    ((integer? index) index)
                                    (else #f))))
          (if (not (and (pair? path) (eq? (car path) '*state*) (pair? (cdr path))))
              '((owner #f))
              `((owner (*state* ,(cadr path)))
                (relative-path ,(cddr path))
                (permission ,permission)
                (index ,(and request-index ((self '~resolve-index) request-index latest-index))))))))

  (define-method (~resolve-authorized? self permission index latest-index)
    ;; Return #t if resolve permission allows a normalized request index.
    (cond ((not index) #f)
          ((eq? permission #t) #t)
          ((not permission) #f)
          (else (let ((start ((self '~resolve-index) (car permission) latest-index))
                      (end ((self '~resolve-index) (cadr permission) latest-index)))
                  (and (<= start index) (<= index end))))))

  (define-method (~resolve-range? self permission latest-index)
    ;; Validate a resolve permission value.
    (cond ((boolean? permission) #t)
          ((not (and (list? permission)
                     (= (length permission) 2)
                     (integer? (car permission))
                     (integer? (cadr permission)))) #f)
          ((and (< (car permission) 0) (>= (cadr permission) 0)) #f)
          ((not latest-index)
           (or (and (>= (car permission) 0) (< (cadr permission) 0))
               (<= (car permission) (cadr permission))))
          (else (<= ((self '~resolve-index) (car permission) latest-index)
                    ((self '~resolve-index) (cadr permission) latest-index)))))

  (define-method (~resolve-index self index latest-index)
    ;; Resolve an absolute or relative index against the current head index.
    (if (< index 0) (+ latest-index 1 index) index))

  (define-method (~normalize-principal self principal)
    ;; Canonical remote principals use concise aliases. Expand only well-formed
    ;; explicit compatibility principals; malformed mixed forms remain invalid.
    (if (not (and (list? principal) (member '*bridge* principal))) principal
        (let loop ((rest principal) (out '()))
          (cond ((null? rest) (reverse out))
                ((and (eq? (car rest) '*bridge*)
                      (pair? (cdr rest))
                      (symbol? (cadr rest))
                      (not (memq (cadr rest)
                                 '(*public* *bridge* *state* *crypto* *transition*))))
                 (loop (cddr rest) (cons (cadr rest) out)))
                ((and (eq? (car rest) '*state*)
                      (pair? (cdr rest))
                      (symbol? (cadr rest))
                      (null? (cddr rest)))
                 (append (reverse out) rest))
                (else principal)))))

  (define-method (~principal? self principal)
    ;; Return #t for supported authorization principals.
    (set! principal ((self '~normalize-principal) principal))
    (or (equal? principal '(*public*))
        ((self '~local-user-principal?) principal)
        (let loop ((rest principal) (aliases 0))
          (cond ((null? rest) (> aliases 0))
                ((not (symbol? (car rest))) #f)
                ((eq? (car rest) '*state*)
                 (and (> aliases 0) (pair? (cdr rest)) (null? (cddr rest))))
                ((memq (car rest) '(*public* *bridge* *crypto* *transition*)) #f)
                (else (loop (cdr rest) (+ aliases 1)))))))

  (define-method (~bridge-principal? self principal name)
    ;; Return #t when a principal begins with the named local bridge edge.
    (set! principal ((self '~normalize-principal) principal))
    (and (pair? principal) (equal? (car principal) name)))

  (define-method (~local-user-principal? self principal)
    ;; Return #t for local user principals.
    (and (pair? principal)
         (eq? (car principal) '*state*)
         (pair? (cdr principal))
         (null? (cddr principal))))

  (define-method (~path-prefix? self prefix path)
    ;; Return #t when prefix is a list prefix of path.
    (or (null? prefix)
        (and (pair? path)
             (equal? (car prefix) (car path))
             ((self '~path-prefix?) (cdr prefix) (cdr path)))))

  (define-method (~path-suffix self prefix path)
    ;; Return the suffix after prefix when prefix matches path, otherwise #f.
    (cond ((null? prefix) path)
          ((or (null? path) (not (equal? (car prefix) (car path)))) #f)
          (else ((self '~path-suffix) (cdr prefix) (cdr path)))))

  (define-method (~contains-pair? self xs)
    ;; Return #t if a list contains a nested pair.
    (and (pair? xs)
         (or (pair? (car xs)) ((self '~contains-pair?) (cdr xs))))))
